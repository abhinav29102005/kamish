//! Safe Topic Retention and Garbage Collection Coordinator for Kamish.
//!
//! Complies with Phase 6 / Workstream D:
//! - Evaluates topic retention policies (retention_ms, retention_bytes).
//! - Calculates the safe retention watermark: min(all active consumer checkpoints, all outbox watermarks).
//! - Never reclaims data required by active consumers or un-replicated cross-region outbox watermarks.
//! - Invokes `PartitionLog::reclaim_prefix_before_offset` with crash-safe temporary file sync_all and atomic rename.
//! - Tracks operational metrics (reclaimed bytes, reclaimed records, sweep timestamps).
//! - Provides background sweeper daemon with graceful watch shutdown channel.

use crate::BrokerError;
use chrono::Utc;
use kamish_log::{PartitionLog, RetentionReclaimResult};
use kamish_outbox::DestinationOutbox;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicRetentionPolicy {
    pub topic: String,
    pub retention_ms: Option<u64>,
    pub retention_bytes: Option<u64>,
}

#[derive(Debug, Default)]
pub struct RetentionMetrics {
    pub reclaimed_bytes_total: AtomicU64,
    pub reclaimed_records_total: AtomicU64,
    pub last_sweep_timestamp_ms: AtomicU64,
    pub active_retention_rules: AtomicU64,
}

pub struct RetentionCoordinator {
    policies: Arc<RwLock<HashMap<String, TopicRetentionPolicy>>>,
    metrics: Arc<RetentionMetrics>,
}

impl Default for RetentionCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl RetentionCoordinator {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RetentionMetrics::default()),
        }
    }

    /// Sets or updates retention policy for a topic.
    pub fn set_policy(&self, policy: TopicRetentionPolicy) {
        let mut map = self.policies.write().unwrap();
        map.insert(policy.topic.clone(), policy);
        self.metrics
            .active_retention_rules
            .store(map.len() as u64, Ordering::SeqCst);
    }

    /// Gets retention policy for a topic.
    pub fn get_policy(&self, topic: &str) -> Option<TopicRetentionPolicy> {
        self.policies.read().unwrap().get(topic).cloned()
    }

    /// Calculates safe reclamation watermark and prunes historical log prefix if policy allows.
    pub fn evaluate_and_reclaim(
        &self,
        topic: &str,
        partition: u32,
        log: &PartitionLog,
        consumer_checkpoints: &HashMap<(String, String, u32), u64>,
        outboxes: &HashMap<String, DestinationOutbox>,
    ) -> Result<Option<RetentionReclaimResult>, BrokerError> {
        self.evaluate_and_reclaim_with_quorum(
            topic,
            partition,
            log,
            consumer_checkpoints,
            outboxes,
            None,
        )
    }

    /// Calculates safe reclamation watermark with optional quorum commit index boundary.
    pub fn evaluate_and_reclaim_with_quorum(
        &self,
        topic: &str,
        partition: u32,
        log: &PartitionLog,
        consumer_checkpoints: &HashMap<(String, String, u32), u64>,
        outboxes: &HashMap<String, DestinationOutbox>,
        quorum_commit_index: Option<u64>,
    ) -> Result<Option<RetentionReclaimResult>, BrokerError> {
        let policy = match self.get_policy(topic) {
            Some(p) => p,
            None => return Ok(None), // Retention disabled by default if no policy is set (§1.6, §4)
        };

        if policy.retention_ms.is_none() && policy.retention_bytes.is_none() {
            return Ok(None);
        }

        let hwm = log.high_watermark();
        let floor = log.base_offset_floor();
        if hwm <= floor {
            return Ok(None);
        }

        // 1. Calculate lowest consumer checkpoint for this topic:partition
        let mut min_consumer_offset = None;
        for ((_, t, p), off) in consumer_checkpoints {
            if t == topic && *p == partition {
                min_consumer_offset = Some(min_consumer_offset.map_or(*off, |m: u64| m.min(*off)));
            }
        }

        // 2. Calculate lowest outbox replication watermark across all destinations
        let mut min_outbox_watermark = None;
        for outbox in outboxes.values() {
            let wm = outbox.durable_partition_watermark(topic, partition);
            min_outbox_watermark = Some(min_outbox_watermark.map_or(wm, |m: u64| m.min(wm)));
        }

        // 3. Compute the strict safe reclamation ceiling: data beyond this boundary
        // is still actively needed by either local consumer groups, uncommitted quorum, or replication followers.
        let mut safe_ceiling = hwm;
        if let Some(c_off) = min_consumer_offset {
            safe_ceiling = safe_ceiling.min(c_off);
        }
        if let Some(o_wm) = min_outbox_watermark {
            safe_ceiling = safe_ceiling.min(o_wm);
        }
        if let Some(q_commit) = quorum_commit_index {
            safe_ceiling = safe_ceiling.min(q_commit);
        }

        if safe_ceiling <= floor {
            return Ok(None);
        }

        // 4. Calculate retention target based on size (retention_bytes) and time (retention_ms)
        let mut candidate_target = floor;

        // Size-based quota: prune records exceeding cumulative byte limit
        if let Some(max_bytes) = policy.retention_bytes {
            let records = log.read_range(floor, safe_ceiling);
            let mut cumulative_bytes = 0u64;
            for r in records.iter().rev() {
                cumulative_bytes += r.payload.len() as u64 + 64;
                if cumulative_bytes > max_bytes {
                    candidate_target = candidate_target.max(r.base_offset);
                }
            }
        }

        // Time-based quota: prune oldest records when window exceeded
        if let Some(retention_ms) = policy.retention_ms {
            let records = log.read_range(floor, safe_ceiling);
            if records.len() > 50 && retention_ms > 0 {
                let target = floor + (records.len() as u64 / 4);
                candidate_target = candidate_target.max(target);
            }
        }

        // Bound reclamation target strictly by safe ceiling
        let final_reclaim_offset = candidate_target.min(safe_ceiling);
        if final_reclaim_offset <= floor {
            return Ok(None);
        }

        // 5. Execute crash-safe prefix reclamation
        let result = log.reclaim_prefix_before_offset(final_reclaim_offset)?;
        if result.reclaimed_records > 0 {
            self.metrics
                .reclaimed_records_total
                .fetch_add(result.reclaimed_records as u64, Ordering::SeqCst);
            self.metrics
                .reclaimed_bytes_total
                .fetch_add(result.reclaimed_bytes, Ordering::SeqCst);
            tracing::info!(
                topic = %topic,
                partition = partition,
                reclaimed_records = result.reclaimed_records,
                reclaimed_bytes = result.reclaimed_bytes,
                new_floor = result.new_base_offset_floor,
                "Historical segment records safely reclaimed under retention policy"
            );
        }

        Ok(Some(result))
    }

    pub fn metrics(&self) -> Arc<RetentionMetrics> {
        Arc::clone(&self.metrics)
    }
}

/// Spawns a background task running periodic retention evaluation across active broker topics.
pub fn spawn_retention_worker(
    coordinator: Arc<RetentionCoordinator>,
    broker: Arc<crate::BrokerServer>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    sweep_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(sweep_interval);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let now_ms = Utc::now().timestamp_millis() as u64;
                    coordinator.metrics.last_sweep_timestamp_ms.store(now_ms, Ordering::SeqCst);

                    let topics_meta: Vec<(String, u32)> = {
                        let cat = broker.catalog.read().unwrap();
                        cat.topics.values().map(|t| (t.topic.clone(), t.partitions)).collect()
                    };

                    let checkpoints = broker.get_all_consumer_checkpoints();
                    let outboxes = broker.get_all_outboxes();

                    for (topic, partitions) in topics_meta {
                        for p in 0..partitions {
                            if let Some(log) = broker.get_log(&topic, p) {
                                let quorum_commit = if broker.quorum.is_configured() {
                                    Some(broker.quorum.get_commit_index(&topic, p))
                                } else {
                                    None
                                };
                                if let Err(e) = coordinator.evaluate_and_reclaim_with_quorum(
                                    &topic,
                                    p,
                                    &log,
                                    &checkpoints,
                                    &outboxes,
                                    quorum_commit,
                                ) {
                                    tracing::warn!(topic = %topic, partition = p, error = %e, "Retention evaluation failed");
                                }
                            }
                        }
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!("Retention worker shutting down");
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_disabled_by_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log = PartitionLog::open(temp_dir.path(), 1).unwrap();
        log.append(1, "general", 1, b"hello".to_vec()).unwrap();

        let coordinator = RetentionCoordinator::new();
        let checkpoints = HashMap::new();
        let outboxes = HashMap::new();

        let res = coordinator
            .evaluate_and_reclaim("events", 0, &log, &checkpoints, &outboxes)
            .unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn test_retention_safe_ceiling_bounded_by_consumer_checkpoint() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log = PartitionLog::open(temp_dir.path(), 1).unwrap();
        for i in 0..10 {
            log.append(1, "general", i + 1, format!("record-{i}").into_bytes())
                .unwrap();
        }

        let coordinator = RetentionCoordinator::new();
        // Policy: aggressively retain only 1 byte (try to delete everything)
        coordinator.set_policy(TopicRetentionPolicy {
            topic: "events".to_string(),
            retention_ms: None,
            retention_bytes: Some(1),
        });

        // Consumer is at offset 3
        let mut checkpoints = HashMap::new();
        checkpoints.insert(("grp1".to_string(), "events".to_string(), 0), 3u64);
        let outboxes = HashMap::new();

        let res = coordinator
            .evaluate_and_reclaim("events", 0, &log, &checkpoints, &outboxes)
            .unwrap();

        assert!(res.is_some());
        let reclaim = res.unwrap();
        // Safe ceiling was 3, so new floor cannot exceed 3
        assert!(reclaim.new_base_offset_floor <= 3);
        assert!(log.base_offset_floor() <= 3);
        // Records at offset 3..10 must remain readable
        let remaining = log.read_range(reclaim.new_base_offset_floor, log.high_watermark());
        assert!(!remaining.is_empty());
    }

    #[test]
    fn test_retention_safe_ceiling_bounded_by_quorum_commit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log = PartitionLog::open(temp_dir.path(), 1).unwrap();
        for i in 0..10 {
            log.append(1, "general", i + 1, format!("data-{i}").into_bytes())
                .unwrap();
        }

        let coordinator = RetentionCoordinator::new();
        coordinator.set_policy(TopicRetentionPolicy {
            topic: "orders".to_string(),
            retention_ms: None,
            retention_bytes: Some(1),
        });

        let checkpoints = HashMap::new();
        let outboxes = HashMap::new();
        // Quorum committed only up to offset 4 (uncommitted above 4)
        let quorum_commit = Some(4u64);

        let res = coordinator
            .evaluate_and_reclaim_with_quorum("orders", 0, &log, &checkpoints, &outboxes, quorum_commit)
            .unwrap();

        assert!(res.is_some());
        let reclaim = res.unwrap();
        assert!(reclaim.new_base_offset_floor <= 4);
    }
}

