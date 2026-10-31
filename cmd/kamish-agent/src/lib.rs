//! Kamish Inter-Region Replication Agent.
//!
//! Complies with `project.md`:
//! - §2.2: Replication agent sender and receiver.
//! - §2.7.3: Level 2 Process 4.0 Replicate Cross-Region.
//! - §2.7.4: Level 3 Process 4.3 Select Route (Permitted Graph First).
//! - §3.1.2: Cross-Region Replication with Durable Outbox and Resume.
//! - §3.4.5: Reference Outbox Transfer pseudocode.
//! - INV-5: Destination log is prefix of source log, offsets match exactly.
//! - INV-6: Retrying an applied range is a no-op.
//! - INV-8: No payload outside legal set or sealed-only mode.

use kamish_broker::{BrokerClient, BrokerServer, ReplicationRequest};
use kamish_meta::LocalReplicationTracker;
use kamish_outbox::OutboxRange;
use kamish_route::{LinkMetrics, PathCandidate, RouteSelector};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Broker error: {0}")]
    Broker(#[from] kamish_broker::BrokerError),

    #[error("Target broker '{0}' not reachable or not registered")]
    TargetNotRegistered(String),

    #[error("No legal route available for class '{class_id}' to target '{target}'")]
    NoLegalRoute { class_id: String, target: String },

    #[error("Bounded replication queue capacity exceeded (limit: {0})")]
    QueueCapacityExceeded(usize),

    #[error("Replication channel closed")]
    QueueClosed,

    #[error("Replication transfer failed: {0}")]
    TransferFailed(String),
}

/// A replication unit submitted to the bounded async queue (§2.2, §3.3.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationTask {
    pub target_region: String,
    pub range: OutboxRange,
    pub attempt: u32,
    pub max_retries: u32,
    pub backoff_ms: u64,
}

impl ReplicationTask {
    pub fn new(target_region: impl Into<String>, range: OutboxRange) -> Self {
        Self {
            target_region: target_region.into(),
            range,
            attempt: 0,
            max_retries: 3,
            backoff_ms: 50,
        }
    }

    pub fn with_retries(mut self, max_retries: u32, backoff_ms: u64) -> Self {
        self.max_retries = max_retries;
        self.backoff_ms = backoff_ms;
        self
    }
}

/// Real-time replication metrics exposed for observability (§4.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationLagMetric {
    pub source_region: String,
    pub target_region: String,
    pub topic: String,
    pub partition: u32,
    pub local_high_watermark: u64,
    pub remote_durable_watermark: u64,
    pub lag_records: u64,
    pub link_rtt_ms: f64,
}

/// Replication summary from a transfer pass.
#[derive(Debug, Clone, Default)]
pub struct TransferSummary {
    pub ranges_transferred: usize,
    pub bytes_transferred: usize,
    pub advanced_watermark: u64,
}

/// Cross-region replication agent coordinator.
pub struct ReplicationAgent {
    pub local_region: String,
    pub local_broker: Arc<BrokerServer>,
    pub local_tracker: Arc<dyn LocalReplicationTracker>,
    pub router: RouteSelector,
    peer_brokers: Arc<RwLock<HashMap<String, Arc<BrokerServer>>>>,
    peer_addresses: Arc<RwLock<HashMap<String, String>>>,
    link_metrics: Arc<RwLock<HashMap<String, LinkMetrics>>>,
    task_tx: tokio::sync::mpsc::Sender<ReplicationTask>,
    task_rx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<ReplicationTask>>>>,
    queue_capacity: usize,
}

impl ReplicationAgent {
    pub fn new(
        local_region: impl Into<String>,
        local_broker: Arc<BrokerServer>,
        local_tracker: Arc<dyn LocalReplicationTracker>,
        router: RouteSelector,
    ) -> Self {
        Self::with_queue_capacity(local_region, local_broker, local_tracker, router, 1024)
    }

    pub fn with_queue_capacity(
        local_region: impl Into<String>,
        local_broker: Arc<BrokerServer>,
        local_tracker: Arc<dyn LocalReplicationTracker>,
        router: RouteSelector,
        capacity: usize,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(capacity);
        Self {
            local_region: local_region.into(),
            local_broker,
            local_tracker,
            router,
            peer_brokers: Arc::new(RwLock::new(HashMap::new())),
            peer_addresses: Arc::new(RwLock::new(HashMap::new())),
            link_metrics: Arc::new(RwLock::new(HashMap::new())),
            task_tx: tx,
            task_rx: Arc::new(tokio::sync::Mutex::new(Some(rx))),
            queue_capacity: capacity,
        }
    }

    /// Registers a peer broker node for cross-region replication transfers.
    pub fn register_peer(&self, region: impl Into<String>, broker: Arc<BrokerServer>) {
        let region_str = region.into();
        self.peer_brokers
            .write()
            .unwrap()
            .insert(region_str.clone(), broker);
        self.link_metrics
            .write()
            .unwrap()
            .entry(region_str)
            .or_default();
    }

    /// Registers a remote peer broker address (e.g. "127.0.0.1:8082") for TCP transfers.
    pub fn register_remote_peer(&self, region: impl Into<String>, addr: impl Into<String>) {
        let region_str = region.into();
        self.peer_addresses
            .write()
            .unwrap()
            .insert(region_str.clone(), addr.into());
        self.link_metrics
            .write()
            .unwrap()
            .entry(region_str)
            .or_default();
    }

    /// Sets simulated link metrics (RTT, packet loss) for slow-link testing (§4.2).
    /// Returns a deduplicated list of all registered remote peer target regions.
    pub fn registered_targets(&self) -> Vec<String> {
        let mut targets = HashSet::new();
        for p in self.peer_brokers.read().unwrap().keys() {
            targets.insert(p.clone());
        }
        for p in self.peer_addresses.read().unwrap().keys() {
            targets.insert(p.clone());
        }
        for p in &self.local_broker.known_peers {
            targets.insert(p.clone());
        }
        let mut v: Vec<String> = targets.into_iter().collect();
        v.sort();
        v
    }

    pub fn set_link_metrics(&self, target_region: &str, metrics: LinkMetrics) {
        self.link_metrics
            .write()
            .unwrap()
            .insert(target_region.to_string(), metrics);
    }

    /// Enqueues a replication task into the bounded async queue, applying non-blocking backpressure if full.
    pub fn try_submit_task(&self, task: ReplicationTask) -> Result<(), AgentError> {
        self.task_tx.try_send(task).map_err(|e| match e {
            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                AgentError::QueueCapacityExceeded(self.queue_capacity)
            }
            tokio::sync::mpsc::error::TrySendError::Closed(_) => AgentError::QueueClosed,
        })
    }

    /// Enqueues a replication task into the bounded async queue, awaiting when capacity is saturated.
    pub async fn submit_task(&self, task: ReplicationTask) -> Result<(), AgentError> {
        self.task_tx
            .send(task)
            .await
            .map_err(|_| AgentError::QueueClosed)
    }

    /// Pulls up to `max_ranges` pending from local outbox and stages them into the bounded async queue.
    pub fn enqueue_pending_outbox_tasks(
        &self,
        target_region: &str,
        max_ranges: usize,
    ) -> Result<usize, AgentError> {
        self.local_broker.refill_outbox(target_region)?;
        let outbox = match self.local_broker.get_outbox(target_region) {
            Some(o) => o,
            None => return Ok(0),
        };
        let pending = outbox.next_transfer_batch(max_ranges);
        let mut count = 0;
        for range in pending {
            let task = ReplicationTask::new(target_region, range);
            self.try_submit_task(task)?;
            count += 1;
        }
        Ok(count)
    }

    /// Dispatches a single replication task against the target peer (TCP or in-memory) with route validation.
    pub async fn execute_task(&self, task: &ReplicationTask) -> Result<bool, AgentError> {
        let metrics = self
            .link_metrics
            .read()
            .unwrap()
            .get(&task.target_region)
            .copied()
            .unwrap_or_default();

        let candidate = PathCandidate {
            target_region: task.target_region.clone(),
            relay_region: None,
            metrics,
        };

        let picked_path =
            self.router
                .pick_path(&task.target_region, &task.range.class_id, &[candidate]);

        if picked_path.is_none() {
            return Err(AgentError::NoLegalRoute {
                class_id: task.range.class_id.clone(),
                target: task.target_region.clone(),
            });
        }

        let rep_req = ReplicationRequest {
            topic: task.range.topic.clone(),
            partition: task.range.partition,
            source_region: self.local_region.clone(),
            target_region: task.target_region.clone(),
            class_id: task.range.class_id.clone(),
            epoch: task.range.epoch,
            from_offset: task.range.start_offset,
            to_offset: task.range.end_offset,
            payload: task.range.payload.clone(),
            is_sealed: task.range.is_sealed,
        };

        // Prefer TCP peer if address is registered
        let tcp_addr = {
            self.peer_addresses
                .read()
                .unwrap()
                .get(&task.target_region)
                .cloned()
        };

        if let Some(addr) = tcp_addr {
            let mut client = BrokerClient::connect(&addr).await.map_err(|e| {
                AgentError::TargetNotRegistered(format!("TCP connect to {}: {}", addr, e))
            })?;
            let res = client
                .replicate(rep_req)
                .await
                .map_err(|e| AgentError::TransferFailed(e.to_string()))?;

            if res.accepted {
                if let Some(outbox) = self.local_broker.get_outbox(&task.target_region) {
                    outbox.advance_partition_watermark(
                        &task.range.topic,
                        task.range.partition,
                        res.applied_watermark,
                    );
                }
                self.local_tracker.update_watermark(
                    &task.range.topic,
                    task.range.partition,
                    &task.target_region,
                    res.applied_watermark,
                );
                return Ok(true);
            } else {
                if res.reason.contains("offset_gap") || res.reason.contains("partial_overlap") {
                    if let Some(outbox) = self.local_broker.get_outbox(&task.target_region) {
                        outbox.advance_partition_watermark(
                            &task.range.topic,
                            task.range.partition,
                            res.applied_watermark,
                        );
                    }
                    self.local_tracker.update_watermark(
                        &task.range.topic,
                        task.range.partition,
                        &task.target_region,
                        res.applied_watermark,
                    );
                } else if res.reason.contains("replication_history_conflict") {
                    error!(
                        target = %task.target_region,
                        topic = %task.range.topic,
                        partition = task.range.partition,
                        offset = task.range.start_offset,
                        "Replication history conflict detected during TCP transfer"
                    );
                }
                warn!(
                    target = %task.target_region,
                    reason = %res.reason,
                    watermark = res.applied_watermark,
                    "Remote broker did not accept replication"
                );
                return Ok(false);
            }
        }

        // Fall back to in-memory peer broker
        let in_mem_peer = {
            self.peer_brokers
                .read()
                .unwrap()
                .get(&task.target_region)
                .cloned()
        };

        if let Some(peer) = in_mem_peer {
            let res = tokio::task::spawn_blocking(move || peer.handle_replication(rep_req))
                .await
                .map_err(|e| AgentError::TransferFailed(e.to_string()))??;
            if res.accepted {
                if let Some(outbox) = self.local_broker.get_outbox(&task.target_region) {
                    outbox.advance_partition_watermark(
                        &task.range.topic,
                        task.range.partition,
                        res.applied_watermark,
                    );
                }
                self.local_tracker.update_watermark(
                    &task.range.topic,
                    task.range.partition,
                    &task.target_region,
                    res.applied_watermark,
                );
                return Ok(true);
            } else {
                if res.reason.contains("offset_gap") || res.reason.contains("partial_overlap") {
                    if let Some(outbox) = self.local_broker.get_outbox(&task.target_region) {
                        outbox.advance_partition_watermark(
                            &task.range.topic,
                            task.range.partition,
                            res.applied_watermark,
                        );
                    }
                    self.local_tracker.update_watermark(
                        &task.range.topic,
                        task.range.partition,
                        &task.target_region,
                        res.applied_watermark,
                    );
                } else if res.reason.contains("replication_history_conflict") {
                    error!(
                        target = %task.target_region,
                        topic = %task.range.topic,
                        partition = task.range.partition,
                        offset = task.range.start_offset,
                        "Replication history conflict detected during TCP transfer"
                    );
                }
                warn!(
                    target = %task.target_region,
                    reason = %res.reason,
                    "In-memory peer broker rejected replication"
                );
                return Ok(false);
            }
        }

        Err(AgentError::TargetNotRegistered(task.target_region.clone()))
    }

    /// Spawns the background retry worker processing tasks from the bounded async queue.
    /// Handles exponential backoff and retries, preserving un-acked ranges in the outbox on persistent failure.
    pub async fn spawn_retry_worker(self: Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        let mut rx = {
            let mut guard = self.task_rx.lock().await;
            guard.take()?
        };

        let handle = tokio::spawn(async move {
            info!("Replication retry worker started");
            let mut target_backoffs: HashMap<
                String,
                (tokio::time::Instant, tokio::time::Duration),
            > = HashMap::new();
            let base_backoff = tokio::time::Duration::from_millis(50);
            let max_backoff = tokio::time::Duration::from_millis(2000);
            let sweep_interval = tokio::time::Duration::from_millis(50);

            loop {
                tokio::select! {
                    maybe_task = rx.recv() => {
                        match maybe_task {
                            Some(mut task) => {
                                let mut completed = false;
                                while task.attempt <= task.max_retries {
                                    match self.execute_task(&task).await {
                                        Ok(true) => {
                                            completed = true;
                                            target_backoffs.remove(&task.target_region);
                                            break;
                                        }
                                        Ok(false) => {
                                            task.attempt += 1;
                                            if task.attempt <= task.max_retries {
                                                let delay = task.backoff_ms * (1 << (task.attempt - 1));
                                                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                                            }
                                        }
                                        Err(e) => {
                                            task.attempt += 1;
                                            if task.attempt <= task.max_retries {
                                                let delay = task.backoff_ms * (1 << (task.attempt - 1));
                                                warn!(
                                                    target = %task.target_region,
                                                    attempt = task.attempt,
                                                    delay_ms = delay,
                                                    error = %e,
                                                    "Replication task failed, retrying with backoff"
                                                );
                                                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                                            } else {
                                                warn!(
                                                    target = %task.target_region,
                                                    error = %e,
                                                    "Replication task exhausted max retries; remaining durable in outbox"
                                                );
                                            }
                                        }
                                    }
                                }
                                if !completed {
                                    let next_delay = target_backoffs
                                        .get(&task.target_region)
                                        .map_or(base_backoff, |(_, prev)| (*prev * 2).min(max_backoff));
                                    target_backoffs.insert(
                                        task.target_region.clone(),
                                        (tokio::time::Instant::now() + next_delay, next_delay),
                                    );
                                    warn!(
                                        target = %task.target_region,
                                        topic = %task.range.topic,
                                        partition = task.range.partition,
                                        start_offset = task.range.start_offset,
                                        backoff_ms = next_delay.as_millis(),
                                        "Replication task not acknowledged; outbox retains range for resume"
                                    );
                                }
                            }
                            None => {
                                info!("Replication task channel closed; stopping retry worker");
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(sweep_interval) => {
                        let targets = self.registered_targets();
                        let now = tokio::time::Instant::now();
                        for target in targets {
                            if let Some((ready_at, _)) = target_backoffs.get(&target)
                                && now < *ready_at
                            {
                                continue;
                            }
                            let _ = self.local_broker.refill_outbox(&target);
                            if let Some(outbox) = self.local_broker.get_outbox(&target) {
                                let pending = outbox.next_transfer_batch(10);
                                for range in pending {
                                    let task = ReplicationTask::new(&target, range);
                                    match self.execute_task(&task).await {
                                        Ok(true) => {
                                            target_backoffs.remove(&target);
                                        }
                                        Ok(false) => {
                                            let next_delay = target_backoffs
                                                .get(&target)
                                                .map_or(base_backoff, |(_, prev)| (*prev * 2).min(max_backoff));
                                            target_backoffs.insert(
                                                target.clone(),
                                                (tokio::time::Instant::now() + next_delay, next_delay),
                                            );
                                            break;
                                        }
                                        Err(e) => {
                                            let next_delay = target_backoffs
                                                .get(&target)
                                                .map_or(base_backoff, |(_, prev)| (*prev * 2).min(max_backoff));
                                            target_backoffs.insert(
                                                target.clone(),
                                                (tokio::time::Instant::now() + next_delay, next_delay),
                                            );
                                            warn!(
                                                target = %target,
                                                error = %e,
                                                backoff_ms = next_delay.as_millis(),
                                                "Sweep replication failed for target, backing off"
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            info!("Replication retry worker stopped");
        });

        Some(handle)
    }

    /// Performs replication handshake with the target broker to synchronize offset cursors upon reconnect.
    /// Inquires the remote durable watermark and advances the local outbox cursor to avoid redundant transfers (INV-5, INV-6).
    pub async fn handshake_and_sync_cursor(
        &self,
        target_region: &str,
        topic: &str,
        partition: u32,
    ) -> Result<u64, AgentError> {
        let tcp_addr = {
            self.peer_addresses
                .read()
                .unwrap()
                .get(target_region)
                .cloned()
        };

        let remote_watermark = if let Some(addr) = tcp_addr {
            let mut client = BrokerClient::connect(&addr).await.map_err(|e| {
                AgentError::TargetNotRegistered(format!("TCP connect to {}: {}", addr, e))
            })?;
            client
                .get_watermark(topic, partition, &self.local_region)
                .await
                .map_err(|e| AgentError::TransferFailed(e.to_string()))?
        } else {
            let in_mem = {
                self.peer_brokers
                    .read()
                    .unwrap()
                    .get(target_region)
                    .cloned()
            };
            if let Some(peer) = in_mem {
                peer.get_watermark(topic, partition, &self.local_region)
            } else {
                return Err(AgentError::TargetNotRegistered(target_region.to_string()));
            }
        };

        // Advance local outbox cursor and trim ranges completely committed remotely (INV-5, INV-6)
        if let Some(outbox) = self.local_broker.get_outbox(target_region) {
            outbox.advance_partition_watermark(topic, partition, remote_watermark);
        }
        self.local_tracker
            .update_watermark(topic, partition, target_region, remote_watermark);

        // Repopulate outbox from disk log if there are committed records ahead of remote watermark
        self.local_broker
            .repopulate_outbox_from_log(topic, partition, target_region)?;

        info!(
            target = %target_region,
            topic = %topic,
            partition = partition,
            remote_watermark = remote_watermark,
            "Handshake completed: synchronized cursor with remote broker"
        );

        Ok(remote_watermark)
    }

    /// Performs a single replication drain pass for a target region (§2.7.3, §3.4.5).
    pub fn step_replication(&self, target_region: &str) -> Result<TransferSummary, AgentError> {
        self.local_broker.refill_outbox(target_region)?;
        let peer_broker = {
            let peers = self.peer_brokers.read().unwrap();
            peers
                .get(target_region)
                .cloned()
                .ok_or_else(|| AgentError::TargetNotRegistered(target_region.to_string()))?
        };

        let outbox = match self.local_broker.get_outbox(target_region) {
            Some(o) => o,
            None => return Ok(TransferSummary::default()),
        };

        let pending_ranges = outbox.next_transfer_batch(50);
        if pending_ranges.is_empty() {
            return Ok(TransferSummary::default());
        }

        let metrics = self
            .link_metrics
            .read()
            .unwrap()
            .get(target_region)
            .copied()
            .unwrap_or_default();

        let mut summary = TransferSummary::default();

        for range in pending_ranges {
            // 1. Route selection with hard policy filter first (§2.2, §3.4.3)
            let direct_candidate = PathCandidate {
                target_region: target_region.to_string(),
                relay_region: None,
                metrics,
            };

            let picked_path =
                self.router
                    .pick_path(target_region, &range.class_id, &[direct_candidate]);

            let _path = match picked_path {
                Some(p) => p,
                None => {
                    warn!(
                        class = %range.class_id,
                        target = %target_region,
                        "No legal replication route permitted by policy, skipping range"
                    );
                    continue;
                }
            };

            // 2. Transfer range to target broker (E4 send / E5 receive)
            let rep_req = ReplicationRequest {
                topic: range.topic.clone(),
                partition: range.partition,
                source_region: self.local_region.clone(),
                target_region: target_region.to_string(),
                class_id: range.class_id.clone(),
                epoch: range.epoch,
                from_offset: range.start_offset,
                to_offset: range.end_offset,
                payload: range.payload.clone(),
                is_sealed: range.is_sealed,
            };

            let rep_res = peer_broker.handle_replication(rep_req)?;
            if rep_res.accepted {
                // 3. Advance local outbox watermark and trim applied range (INV-5, INV-6)
                outbox.advance_partition_watermark(
                    &range.topic,
                    range.partition,
                    rep_res.applied_watermark,
                );
                self.local_tracker.update_watermark(
                    &range.topic,
                    range.partition,
                    target_region,
                    rep_res.applied_watermark,
                );

                summary.ranges_transferred += 1;
                summary.bytes_transferred += range.byte_size();
                summary.advanced_watermark = rep_res.applied_watermark;
            } else if rep_res.reason.contains("offset_gap")
                || rep_res.reason.contains("partial_overlap")
            {
                outbox.advance_partition_watermark(
                    &range.topic,
                    range.partition,
                    rep_res.applied_watermark,
                );
                self.local_tracker.update_watermark(
                    &range.topic,
                    range.partition,
                    target_region,
                    rep_res.applied_watermark,
                );
                if rep_res.applied_watermark > summary.advanced_watermark {
                    summary.advanced_watermark = rep_res.applied_watermark;
                }
                warn!(
                    target = %target_region,
                    topic = %range.topic,
                    partition = range.partition,
                    reason = %rep_res.reason,
                    applied_watermark = rep_res.applied_watermark,
                    "Replication rejected due to gap or partial overlap; halting batch to resume from confirmed prefix"
                );
                break;
            } else if rep_res.reason.contains("replication_history_conflict") {
                error!(
                    target = %target_region,
                    topic = %range.topic,
                    partition = range.partition,
                    offset = range.start_offset,
                    "Replication rejected due to history conflict; stopping transfer batch"
                );
                break;
            } else {
                warn!(
                    target = %target_region,
                    topic = %range.topic,
                    partition = range.partition,
                    reason = %rep_res.reason,
                    "Replication rejected by peer"
                );
            }
        }

        info!(
            target = %target_region,
            transferred = summary.ranges_transferred,
            watermark = summary.advanced_watermark,
            "Replication step completed"
        );

        Ok(summary)
    }

    /// Performs a replication drain pass connecting over TCP to a remote broker daemon.
    pub async fn step_replication_tcp(
        &self,
        target_region: &str,
    ) -> Result<TransferSummary, AgentError> {
        self.local_broker.refill_outbox(target_region)?;
        let addr = {
            let addrs = self.peer_addresses.read().unwrap();
            addrs
                .get(target_region)
                .cloned()
                .ok_or_else(|| AgentError::TargetNotRegistered(target_region.to_string()))?
        };

        let outbox = match self.local_broker.get_outbox(target_region) {
            Some(o) => o,
            None => return Ok(TransferSummary::default()),
        };

        let pending_ranges = outbox.next_transfer_batch(50);
        if pending_ranges.is_empty() {
            return Ok(TransferSummary::default());
        }

        let metrics = self
            .link_metrics
            .read()
            .unwrap()
            .get(target_region)
            .copied()
            .unwrap_or_default();

        let mut client = BrokerClient::connect(&addr).await.map_err(|e| {
            AgentError::TargetNotRegistered(format!("TCP connect to {}: {}", addr, e))
        })?;

        let mut summary = TransferSummary::default();

        for range in pending_ranges {
            let direct_candidate = PathCandidate {
                target_region: target_region.to_string(),
                relay_region: None,
                metrics,
            };

            let picked_path =
                self.router
                    .pick_path(target_region, &range.class_id, &[direct_candidate]);

            if picked_path.is_none() {
                continue;
            }

            let rep_req = ReplicationRequest {
                topic: range.topic.clone(),
                partition: range.partition,
                source_region: self.local_region.clone(),
                target_region: target_region.to_string(),
                class_id: range.class_id.clone(),
                epoch: range.epoch,
                from_offset: range.start_offset,
                to_offset: range.end_offset,
                payload: range.payload.clone(),
                is_sealed: range.is_sealed,
            };

            let start = std::time::Instant::now();
            let rep_res = client
                .replicate(rep_req)
                .await
                .map_err(|e| AgentError::TargetNotRegistered(e.to_string()))?;
            let rtt_ms = start.elapsed().as_secs_f64() * 1000.0;

            if let Some(m) = self.link_metrics.write().unwrap().get_mut(target_region) {
                m.rtt_ms = (m.rtt_ms * 0.8) + (rtt_ms * 0.2);
            }

            if rep_res.accepted {
                outbox.advance_partition_watermark(
                    &range.topic,
                    range.partition,
                    rep_res.applied_watermark,
                );
                self.local_tracker.update_watermark(
                    &range.topic,
                    range.partition,
                    target_region,
                    rep_res.applied_watermark,
                );

                summary.ranges_transferred += 1;
                summary.bytes_transferred += range.byte_size();
                summary.advanced_watermark = rep_res.applied_watermark;
            } else if rep_res.reason.contains("offset_gap")
                || rep_res.reason.contains("partial_overlap")
            {
                outbox.advance_partition_watermark(
                    &range.topic,
                    range.partition,
                    rep_res.applied_watermark,
                );
                self.local_tracker.update_watermark(
                    &range.topic,
                    range.partition,
                    target_region,
                    rep_res.applied_watermark,
                );
                if rep_res.applied_watermark > summary.advanced_watermark {
                    summary.advanced_watermark = rep_res.applied_watermark;
                }
                warn!(
                    target = %target_region,
                    topic = %range.topic,
                    partition = range.partition,
                    reason = %rep_res.reason,
                    applied_watermark = rep_res.applied_watermark,
                    "Replication rejected due to gap or partial overlap; halting batch to resume from confirmed prefix"
                );
                break;
            } else if rep_res.reason.contains("replication_history_conflict") {
                error!(
                    target = %target_region,
                    topic = %range.topic,
                    partition = range.partition,
                    offset = range.start_offset,
                    "Replication rejected due to history conflict; stopping transfer batch"
                );
                break;
            } else {
                warn!(
                    target = %target_region,
                    topic = %range.topic,
                    partition = range.partition,
                    reason = %rep_res.reason,
                    "Replication rejected by peer"
                );
            }
        }

        Ok(summary)
    }

    /// Measures and returns real-time replication lag for observability (§4.4).
    pub fn get_replication_lag(
        &self,
        target_region: &str,
        topic: &str,
        partition: u32,
        local_hwm: u64,
    ) -> ReplicationLagMetric {
        let remote_watermark = match self.local_broker.get_outbox(target_region) {
            Some(o) => o.durable_partition_watermark(topic, partition),
            None => 0,
        };

        let lag = local_hwm.saturating_sub(remote_watermark);
        let rtt = self
            .link_metrics
            .read()
            .unwrap()
            .get(target_region)
            .map(|m| m.rtt_ms)
            .unwrap_or(0.0);

        ReplicationLagMetric {
            source_region: self.local_region.clone(),
            target_region: target_region.to_string(),
            topic: topic.to_string(),
            partition,
            local_high_watermark: local_hwm,
            remote_durable_watermark: remote_watermark,
            lag_records: lag,
            link_rtt_ms: rtt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use kamish_audit::AuditLog;
    use kamish_broker::ProduceRequest;
    use kamish_meta::MetadataStore;
    use kamish_policy::{
        AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
    };
    use rand::rngs::OsRng;
    use std::collections::HashSet;

    fn setup_agent_environment() -> (
        ReplicationAgent,
        Arc<BrokerServer>,
        Arc<BrokerServer>,
        tempfile::TempDir,
        tempfile::TempDir,
    ) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert("region-eu".to_string());
        legal.insert("region-us".to_string());

        let mut modes = HashMap::new();
        modes.insert("region-eu".to_string(), DestinationMode::Cleartext);
        modes.insert("region-us".to_string(), DestinationMode::Cleartext);

        let mut classes = HashMap::new();
        classes.insert(
            "global-stream".to_string(),
            ClassRule {
                class_id: "global-stream".to_string(),
                description: "Cross region allowed".to_string(),
                legal_regions: legal,
                destination_modes: modes,
                ack_mode: AckMode::LocalMajority,
                failover_candidates: vec!["region-eu".to_string(), "region-us".to_string()],
                transit_allowed: true,
            },
        );

        let policy_doc = PolicyDocument {
            policy_id: "pol-agent".to_string(),
            version: 1,
            issued_at_sec: Utc::now().timestamp() - 5,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes,
            transit_rules: HashMap::new(),
        };

        let signed = SignedPolicy::sign(policy_doc, &signing_key);
        let mut trusted = HashSet::new();
        trusted.insert(pubkey_hex);

        let mut cache = PolicyCache::new(trusted);
        cache.update(signed, Utc::now().timestamp()).unwrap();
        let cache_arc = Arc::new(RwLock::new(cache));

        let audit_a = AuditLog::new_in_memory();
        let audit_b = AuditLog::new_in_memory();

        let enforcer_a = Arc::new(kamish_enforce::Enforcer::new(
            "region-eu",
            cache_arc.clone(),
            audit_a,
        ));
        let enforcer_b = Arc::new(kamish_enforce::Enforcer::new(
            "region-us",
            cache_arc,
            audit_b,
        ));

        let meta_a = Arc::new(MetadataStore::new());
        let meta_b = Arc::new(MetadataStore::new());

        let td_a = tempfile::tempdir().unwrap();
        let td_b = tempfile::tempdir().unwrap();

        let broker_a = Arc::new(BrokerServer::new(
            "region-eu",
            enforcer_a.clone(),
            meta_a.clone(),
            vec!["region-us".to_string()],
            td_a.path(),
        ));
        let broker_b = Arc::new(BrokerServer::new(
            "region-us",
            enforcer_b,
            meta_b,
            vec!["region-eu".to_string()],
            td_b.path(),
        ));

        let router = RouteSelector::new("region-eu", (*enforcer_a).clone());
        let agent = ReplicationAgent::new("region-eu", broker_a.clone(), meta_a, router);
        agent.register_peer("region-us", broker_b.clone());

        (agent, broker_a, broker_b, td_a, td_b)
    }

    #[test]
    fn test_agent_replication_and_lag_monitoring() {
        let (agent, broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();

        // 1. Ingest 4 records into Broker A
        for seq in 0..4 {
            let req = ProduceRequest {
                topic: "orders".to_string(),
                partition: 0,
                producer_id: 8001,
                producer_epoch: 1,
                sequence_base: seq,
                class_id: "global-stream".to_string(),
                payload: format!("payload-{}", seq).into_bytes(),
                residency_rule: None,
            };
            broker_a.produce(req).unwrap();
        }

        // Check initial lag: 4 records
        let lag_metric = agent.get_replication_lag("region-us", "orders", 0, 4);
        assert_eq!(lag_metric.lag_records, 4);
        assert_eq!(lag_metric.remote_durable_watermark, 0);

        // 2. Step replication agent: drains outbox and replicates to Broker B
        let summary = agent.step_replication("region-us").unwrap();
        assert_eq!(summary.ranges_transferred, 4);
        assert_eq!(summary.advanced_watermark, 4);

        // 3. Lag is now 0!
        let lag_metric_after = agent.get_replication_lag("region-us", "orders", 0, 4);
        assert_eq!(lag_metric_after.lag_records, 0);
        assert_eq!(lag_metric_after.remote_durable_watermark, 4);

        // 4. Broker B has all 4 records stored in exact offset order (INV-5)
        let fetch_res = broker_b
            .fetch(kamish_broker::FetchRequest {
                topic: "orders".to_string(),
                partition: 0,
                class_id: "global-stream".to_string(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap();
        assert_eq!(fetch_res.records.len(), 4);
        assert_eq!(fetch_res.records[3].payload, b"payload-3");
    }

    #[test]
    fn test_agent_simulated_slow_link() {
        let (agent, broker_a, _broker_b, _td_a, _td_b) = setup_agent_environment();

        // Simulate a degraded slow link: 350ms RTT, 5% loss
        agent.set_link_metrics(
            "region-us",
            LinkMetrics {
                rtt_ms: 350.0,
                loss_rate: 0.05,
                queue_delay_ms: 50.0,
                cost_weight: 1.0,
                congestion_risk: 0.4,
            },
        );

        // Ingest into Broker A
        broker_a
            .produce(ProduceRequest {
                topic: "events".to_string(),
                partition: 0,
                producer_id: 9001,
                producer_epoch: 1,
                sequence_base: 0,
                class_id: "global-stream".to_string(),
                payload: b"event-data".to_vec(),
                residency_rule: None,
            })
            .unwrap();

        // Lag check reflects the slow link RTT accurately
        let lag = agent.get_replication_lag("region-us", "events", 0, 1);
        assert_eq!(lag.lag_records, 1);
        assert_eq!(lag.link_rtt_ms, 350.0);
    }

    #[tokio::test]
    async fn test_bounded_replication_queue_and_backpressure() {
        let (agent, _broker_a, _broker_b, _td_a, _td_b) = setup_agent_environment();
        // Create an agent with capacity of 2
        let agent = Arc::new(ReplicationAgent::with_queue_capacity(
            agent.local_region.clone(),
            agent.local_broker.clone(),
            agent.local_tracker.clone(),
            agent.router.clone(),
            2,
        ));

        let task1 = ReplicationTask::new(
            "region-us",
            OutboxRange {
                topic: "t1".to_string(),
                partition: 0,
                epoch: 1,
                start_offset: 0,
                end_offset: 1,
                class_id: "global-stream".to_string(),
                payload: b"p1".to_vec(),
                is_sealed: false,
            },
        );
        let task2 = ReplicationTask::new(
            "region-us",
            OutboxRange {
                topic: "t1".to_string(),
                partition: 0,
                epoch: 1,
                start_offset: 1,
                end_offset: 2,
                class_id: "global-stream".to_string(),
                payload: b"p2".to_vec(),
                is_sealed: false,
            },
        );
        let task3 = ReplicationTask::new(
            "region-us",
            OutboxRange {
                topic: "t1".to_string(),
                partition: 0,
                epoch: 1,
                start_offset: 2,
                end_offset: 3,
                class_id: "global-stream".to_string(),
                payload: b"p3".to_vec(),
                is_sealed: false,
            },
        );

        // First 2 succeed
        assert!(agent.try_submit_task(task1).is_ok());
        assert!(agent.try_submit_task(task2).is_ok());

        // 3rd task exceeds bounded capacity -> returns QueueCapacityExceeded(2)
        let err = agent.try_submit_task(task3).unwrap_err();
        match err {
            AgentError::QueueCapacityExceeded(cap) => assert_eq!(cap, 2),
            other => panic!("Expected QueueCapacityExceeded, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_retry_worker_with_backoff_and_recovery() {
        let (agent, broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();
        let agent = Arc::new(agent);

        // Ingest into Broker A
        broker_a
            .produce(ProduceRequest {
                topic: "events".to_string(),
                partition: 0,
                producer_id: 100,
                producer_epoch: 1,
                sequence_base: 0,
                class_id: "global-stream".to_string(),
                payload: b"first-event".to_vec(),
                residency_rule: None,
            })
            .unwrap();

        // Enqueue from outbox to async bounded queue
        let enqueued = agent.enqueue_pending_outbox_tasks("region-us", 10).unwrap();
        assert_eq!(enqueued, 1);

        // Spawn retry worker
        let worker_handle = agent.clone().spawn_retry_worker().await.unwrap();

        // Allow retry worker to process the task
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

        // Verify watermark advanced to 1 and records exist in broker B
        let outbox = broker_a.get_outbox("region-us").unwrap();
        assert_eq!(outbox.durable_watermark(), 1);

        let fetch = broker_b
            .fetch(kamish_broker::FetchRequest {
                topic: "events".to_string(),
                partition: 0,
                class_id: "global-stream".to_string(),
                start_offset: 0,
                max_records: 5,
            })
            .unwrap();
        assert_eq!(fetch.records.len(), 1);
        assert_eq!(fetch.records[0].payload, b"first-event");

        worker_handle.abort();
    }

    #[tokio::test]
    async fn test_offset_cursor_synchronization_and_deduplication_on_reconnect() {
        let (agent, broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();

        // 1. Ingest 4 records [0, 1, 2, 3] into Broker A
        for i in 0..4 {
            broker_a
                .produce(ProduceRequest {
                    topic: "orders".to_string(),
                    partition: 0,
                    producer_id: 200,
                    producer_epoch: 1,
                    sequence_base: i,
                    class_id: "global-stream".to_string(),
                    payload: format!("order-{}", i).into_bytes(),
                    residency_rule: None,
                })
                .unwrap();
        }

        // 2. Broker B manually receives records 0 and 1 (simulating transfer before partition)
        // Broker B now has watermark 2
        let rep0 = ReplicationRequest {
            topic: "orders".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 0,
            to_offset: 1,
            payload: b"order-0".to_vec(),
            is_sealed: false,
        };
        let rep1 = ReplicationRequest {
            topic: "orders".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 1,
            to_offset: 2,
            payload: b"order-1".to_vec(),
            is_sealed: false,
        };
        assert!(broker_b.handle_replication(rep0).unwrap().accepted);
        assert!(broker_b.handle_replication(rep1).unwrap().accepted);

        // But broker A's outbox watermark is still 0 (e.g. ACKs dropped in WAN partition)
        let outbox = broker_a.get_outbox("region-us").unwrap();
        assert_eq!(outbox.durable_watermark(), 0);

        // 3. Reconnect! ReplicationAgent runs handshake_and_sync_cursor
        let synced_watermark = agent
            .handshake_and_sync_cursor("region-us", "orders", 0)
            .await
            .unwrap();
        assert_eq!(synced_watermark, 2);

        // Local outbox cursor is now synchronized to 2, and records 0..2 are trimmed!
        assert_eq!(outbox.durable_watermark(), 2);
        let pending = outbox.next_transfer_batch(10);
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].start_offset, 2);
        assert_eq!(pending[1].start_offset, 3);

        // 4. Test deduplication on reconnect (INV-6):
        // Retransmit the already-committed record at [0..1]. The wire format is
        // one record per request; a claimed two-record range is not a valid retry.
        let dup_req = ReplicationRequest {
            topic: "orders".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 0,
            to_offset: 1,
            payload: b"order-0".to_vec(),
            is_sealed: false,
        };
        let dup_res = broker_b.handle_replication(dup_req).unwrap();
        assert!(dup_res.accepted);
        assert_eq!(dup_res.reason, "idempotent_duplicate_noop");
        assert_eq!(dup_res.applied_watermark, 2);

        // 5. Transfer remaining records [2, 3]
        let summary = agent.step_replication("region-us").unwrap();
        assert_eq!(summary.ranges_transferred, 2);
        assert_eq!(summary.advanced_watermark, 4);

        // 6. Broker B has all 4 records with dense continuous monotonic offsets [0, 1, 2, 3] and 0 duplicate writes!
        let fetched = broker_b
            .fetch(kamish_broker::FetchRequest {
                topic: "orders".to_string(),
                partition: 0,
                class_id: "global-stream".to_string(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap();
        assert_eq!(fetched.records.len(), 4);
        for (idx, rec) in fetched.records.iter().enumerate() {
            assert_eq!(rec.base_offset, idx as u64);
            assert_eq!(rec.payload, format!("order-{}", idx).into_bytes());
        }
    }

    #[tokio::test]
    async fn test_destination_persistence_followed_by_lost_ack() {
        let (agent, broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();

        // 1. Ingest record at Broker A
        let p_res = broker_a
            .produce(ProduceRequest {
                topic: "adversarial-stream".to_string(),
                partition: 0,
                producer_id: 101,
                producer_epoch: 1,
                sequence_base: 0,
                class_id: "global-stream".to_string(),
                payload: b"payload-0".to_vec(),
                residency_rule: None,
            })
            .unwrap();
        assert_eq!(p_res.base_offset, 0);

        // 2. Broker B receives replication request directly and persists it
        let req0 = ReplicationRequest {
            topic: "adversarial-stream".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 0,
            to_offset: 1,
            payload: b"payload-0".to_vec(),
            is_sealed: false,
        };
        let r_res = broker_b.handle_replication(req0.clone()).unwrap();
        assert!(r_res.accepted);
        assert_eq!(r_res.applied_watermark, 1);

        // 3. Simulate lost ACK: Broker A's outbox watermark was NOT advanced
        let outbox = broker_a.get_outbox("region-us").unwrap();
        assert_eq!(outbox.durable_watermark(), 0);

        // 4. Broker A retries the exact same range [0..1]
        let retry_res = broker_b.handle_replication(req0).unwrap();
        assert!(
            retry_res.accepted,
            "Idempotent duplicate retry must be accepted"
        );
        assert_eq!(retry_res.reason, "idempotent_duplicate_noop");
        assert_eq!(retry_res.applied_watermark, 1);

        // 5. Verify Receiver B has exactly 1 record on disk (zero duplicate writes)
        let fetched = broker_b
            .fetch(kamish_broker::FetchRequest {
                topic: "adversarial-stream".to_string(),
                partition: 0,
                class_id: "global-stream".to_string(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap();
        assert_eq!(fetched.records.len(), 1);
        assert_eq!(fetched.records[0].base_offset, 0);
        assert_eq!(fetched.records[0].payload, b"payload-0");

        // 6. Sync cursor on Agent side
        let synced = agent
            .handshake_and_sync_cursor("region-us", "adversarial-stream", 0)
            .await
            .unwrap();
        assert_eq!(synced, 1);
        assert_eq!(outbox.durable_watermark(), 1);
        assert_eq!(outbox.next_transfer_batch(10).len(), 0);
    }

    #[test]
    fn test_missing_offset_range_gap_rejection() {
        let (_agent, _broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();

        // Receiver B is at offset 0. Sender sends range starting at offset 5 (gap of 5 messages)
        let gap_req = ReplicationRequest {
            topic: "gap-stream".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 5,
            to_offset: 6,
            payload: b"premature-message".to_vec(),
            is_sealed: false,
        };

        let res = broker_b.handle_replication(gap_req).unwrap();
        assert!(
            !res.accepted,
            "Receiver must reject non-contiguous offset gap"
        );
        assert!(res.reason.contains("offset_gap"));
        assert_eq!(res.applied_watermark, 0);

        // Verify Receiver log remains empty
        let fetched = broker_b
            .fetch(kamish_broker::FetchRequest {
                topic: "gap-stream".to_string(),
                partition: 0,
                class_id: "global-stream".to_string(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap();
        assert_eq!(fetched.records.len(), 0);
    }

    #[test]
    fn test_partial_overlap_resynchronization() {
        let (_agent, _broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();

        // Ingest records 0 and 1 into Broker B
        for i in 0..2 {
            let req = ReplicationRequest {
                topic: "overlap-stream".to_string(),
                partition: 0,
                source_region: "region-eu".to_string(),
                target_region: "region-us".to_string(),
                class_id: "global-stream".to_string(),
                epoch: 1,
                from_offset: i,
                to_offset: i + 1,
                payload: format!("msg-{}", i).into_bytes(),
                is_sealed: false,
            };
            assert!(broker_b.handle_replication(req).unwrap().accepted);
        }

        // Now submit a partially overlapping range: from_offset: 1, to_offset: 3
        let overlap_req = ReplicationRequest {
            topic: "overlap-stream".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 1,
            to_offset: 3,
            payload: b"overlap-payload".to_vec(),
            is_sealed: false,
        };

        let res = broker_b.handle_replication(overlap_req).unwrap();
        assert!(!res.accepted);
        assert!(res.reason.contains("partial_overlap"));
        assert_eq!(res.applied_watermark, 2);
    }

    #[test]
    fn test_stale_replication_worker_after_epoch_change() {
        let (_agent, _broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();

        // Ingest initial record at epoch 2
        let req_epoch2 = ReplicationRequest {
            topic: "epoch-stream".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 2,
            from_offset: 0,
            to_offset: 1,
            payload: b"epoch-2-record".to_vec(),
            is_sealed: false,
        };
        assert!(broker_b.handle_replication(req_epoch2).unwrap().accepted);

        // Stale worker tries to append with epoch 1
        let req_stale = ReplicationRequest {
            topic: "epoch-stream".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 1,
            to_offset: 2,
            payload: b"stale-write".to_vec(),
            is_sealed: false,
        };
        let response = broker_b.handle_replication(req_stale).unwrap();
        assert!(!response.accepted);
        assert_eq!(response.applied_watermark, 1);
        assert_eq!(response.reason, "replication_target_or_epoch_mismatch");
    }
}
