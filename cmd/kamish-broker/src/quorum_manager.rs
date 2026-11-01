//! In-Region Replicated Log Consensus and Synchronous Quorum Durability Engine.
//!
//! Complies with:
//! - §3.1.1, CONTRACT.md §3.2, ADR-0008, INV-2, INV-3, INV-4, W04, W20
//! - Option 1: Exactly 3 regional replica nodes participating in primary-backup majority replication.
//! - Strict majority quorum: ACK requires local fsync + durable fsync on at least M = floor(V / 2) + 1 voting replicas.
//! - Follower log divergence quarantine: divergent uncommitted suffixes are isolated into QuarantineStore
//!   before truncation, preserving committed authoritative prefixes.
//! - Consumer isolation (INV-4): fetch operations under InRegionQuorum mode are strictly bounded by
//!   the quorum commit index, preventing dirty reads of speculative suffixes.
//! - Restart recovery: persistent terms, votes, and commit indices stored in `consensus_state.json`.

use crate::{BrokerClient, BrokerError, BrokerRequest, BrokerResponse, BrokerServer};
use chrono::Utc;
use kamish_log::RecordEnvelope;
use kamish_own::OwnershipError;
use kamish_own::quorum::{PersistentConsensusState, ReplicaGroupConfig, ReplicaRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::timeout;

/// In-region append request sent by partition leader to voting followers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumAppendRequest {
    pub term: u64,
    pub leader_id: String,
    pub topic: String,
    pub partition: u32,
    pub epoch: u64,
    pub base_offset: u64,
    pub records: Vec<RecordEnvelope>,
    pub leader_commit: u64,
}

/// Acknowledgment response sent by follower to partition leader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumAppendResponse {
    pub term: u64,
    pub node_id: String,
    pub topic: String,
    pub partition: u32,
    pub success: bool,
    pub match_offset: u64,
    pub error: Option<String>,
}

/// Periodic heartbeat request maintaining follower lease and advancing commit indices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumHeartbeatRequest {
    pub term: u64,
    pub leader_id: String,
    pub commit_indices: HashMap<String, u64>,
}

/// Response to quorum heartbeat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumHeartbeatResponse {
    pub term: u64,
    pub node_id: String,
    pub success: bool,
    pub high_watermarks: HashMap<String, u64>,
}

/// Request for leader election vote during partition or term transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumVoteRequest {
    pub term: u64,
    pub candidate_id: String,
    pub last_log_term: u64,
    pub last_log_offset: u64,
}

/// Response to vote request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumVoteResponse {
    pub term: u64,
    pub node_id: String,
    pub vote_granted: bool,
    pub reason: Option<String>,
}

/// Live peer telemetry status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReplicaStatus {
    pub node_id: String,
    pub address: String,
    pub is_voter: bool,
    pub connected: bool,
    pub last_ack_ms: i64,
    pub match_offset: u64,
}

/// Detailed operational status of the regional quorum engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumStatusResponse {
    pub configured: bool,
    pub group_id: Option<String>,
    pub region: String,
    pub local_node_id: String,
    pub role: ReplicaRole,
    pub current_term: u64,
    pub leader_id: Option<String>,
    pub quorum_size: usize,
    pub total_voters: usize,
    pub ack_level: crate::AckLevel,
    pub commit_indices: HashMap<String, u64>,
    pub peers: Vec<PeerReplicaStatus>,
}

/// Core in-region quorum consensus and synchronous replication coordinator.
pub struct QuorumReplicationManager {
    config: Option<ReplicaGroupConfig>,
    state: Arc<RwLock<PersistentConsensusState>>,
    role: Arc<RwLock<ReplicaRole>>,
    data_dir: PathBuf,
    default_ack_level: crate::AckLevel,
    peers_telemetry: Arc<RwLock<HashMap<String, PeerReplicaStatus>>>,
    pub quorum_appends_total: AtomicU64,
    pub quorum_appends_failed: AtomicU64,
}

impl QuorumReplicationManager {
    /// Initializes the manager from optional group configuration and persistent disk state.
    pub fn new(
        config: Option<ReplicaGroupConfig>,
        data_dir: impl AsRef<Path>,
        default_ack_level: crate::AckLevel,
    ) -> Result<Self, std::io::Error> {
        let dir = data_dir.as_ref().to_path_buf();
        let loaded_state = PersistentConsensusState::load_from_dir(&dir)?;

        if let Some(ref cfg) = config {
            cfg.validate().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Invalid replica group config: {e}"),
                )
            })?;
        }

        let mut telemetry = HashMap::new();
        if let Some(ref cfg) = config {
            for peer in &cfg.peers {
                telemetry.insert(
                    peer.node_id.clone(),
                    PeerReplicaStatus {
                        node_id: peer.node_id.clone(),
                        address: peer.address.clone(),
                        is_voter: peer.is_voter,
                        connected: false,
                        last_ack_ms: 0,
                        match_offset: 0,
                    },
                );
            }
        }

        let initial_role = if config.is_none() {
            ReplicaRole::Leader
        } else {
            ReplicaRole::Follower
        };

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(loaded_state)),
            role: Arc::new(RwLock::new(initial_role)),
            data_dir: dir,
            default_ack_level,
            peers_telemetry: Arc::new(RwLock::new(telemetry)),
            quorum_appends_total: AtomicU64::new(0),
            quorum_appends_failed: AtomicU64::new(0),
        })
    }

    pub fn is_configured(&self) -> bool {
        self.config.is_some()
    }

    pub fn role(&self) -> ReplicaRole {
        *self.role.read().unwrap()
    }

    pub fn set_role(&self, role: ReplicaRole) {
        *self.role.write().unwrap() = role;
    }

    pub fn current_term(&self) -> u64 {
        self.state.read().unwrap().current_term
    }

    pub fn group_config(&self) -> Option<&ReplicaGroupConfig> {
        self.config.as_ref()
    }

    pub fn local_node_id(&self) -> &str {
        self.config
            .as_ref()
            .map(|c| c.local_node_id.as_str())
            .unwrap_or("standalone-node")
    }

    /// Returns the quorum-committed index for the specified partition.
    pub fn get_commit_index(&self, topic: &str, partition: u32) -> u64 {
        self.state
            .read()
            .unwrap()
            .get_commit_index(topic, partition)
    }

    /// Advances the commit index monotonically and persists consensus state to disk.
    pub fn advance_commit_index(
        &self,
        topic: &str,
        partition: u32,
        index: u64,
    ) -> Result<bool, std::io::Error> {
        let mut state = self.state.write().unwrap();
        if state.advance_commit_index(topic, partition, index) {
            state.save_to_dir(&self.data_dir)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Handles follower receipt of a `QuorumAppendRequest`.
    pub fn handle_quorum_append(
        &self,
        req: QuorumAppendRequest,
        broker: &BrokerServer,
    ) -> Result<QuorumAppendResponse, BrokerError> {
        let local_node = self.local_node_id().to_string();

        // 1. Term fencing (§3.4.2): reject appends under obsolete term
        let (current_term, term_valid) = {
            let mut state = self.state.write().unwrap();
            if req.term < state.current_term {
                (state.current_term, false)
            } else {
                if req.term > state.current_term {
                    state.current_term = req.term;
                    state.leader_id = Some(req.leader_id.clone());
                    *self.role.write().unwrap() = ReplicaRole::Follower;
                    let _ = state.save_to_dir(&self.data_dir);
                }
                (state.current_term, true)
            }
        };

        if !term_valid {
            return Ok(QuorumAppendResponse {
                term: current_term,
                node_id: local_node,
                topic: req.topic,
                partition: req.partition,
                success: false,
                match_offset: 0,
                error: Some(format!(
                    "Stale consensus term: request term {} < follower term {}",
                    req.term, current_term
                )),
            });
        }

        // 2. Ownership epoch fencing
        let log = broker.get_or_create_log(&req.topic, req.partition, req.epoch)?;
        if req.epoch < log.current_epoch() {
            return Ok(QuorumAppendResponse {
                term: current_term,
                node_id: local_node,
                topic: req.topic,
                partition: req.partition,
                success: false,
                match_offset: log.high_watermark(),
                error: Some(format!(
                    "Stale owner epoch: request epoch {} < follower epoch {}",
                    req.epoch,
                    log.current_epoch()
                )),
            });
        }

        // 3. Divergent history reconciliation (§3.1.4, W20)
        let local_hwm = log.high_watermark();
        let local_commit = self.get_commit_index(&req.topic, req.partition);
        if req.base_offset < local_commit {
            return Ok(QuorumAppendResponse {
                term: current_term,
                node_id: local_node,
                topic: req.topic,
                partition: req.partition,
                success: false,
                match_offset: local_hwm,
                error: Some(format!(
                    "Consensus safety violation: leader attempts to truncate committed history (base_offset {} < local commit_index {})",
                    req.base_offset, local_commit
                )),
            });
        }

        if req.base_offset < local_hwm {
            let existing_records = log.read_range(req.base_offset, local_hwm);
            let mut diverged = false;

            for (incoming, existing) in req.records.iter().zip(existing_records.iter()) {
                if incoming.base_offset != existing.base_offset
                    || incoming.checksum != existing.checksum
                {
                    diverged = true;
                    break;
                }
            }

            if diverged || existing_records.len() > req.records.len() {
                tracing::warn!(
                    topic = %req.topic,
                    partition = req.partition,
                    base_offset = req.base_offset,
                    local_hwm = local_hwm,
                    "History divergence detected on follower; isolating divergent suffix to quarantine"
                );
                let count = existing_records.len();
                broker.quarantine.quarantine_records(existing_records)?;
                tracing::info!(
                    quarantined = count,
                    "Quarantined conflicting local follower records before truncation"
                );
                log.truncate_to_offset(req.base_offset)?;
            }
        }

        // 4. Append and fsync incoming records locally
        for record in req.records {
            if record.base_offset >= log.high_watermark() {
                log.append_envelope(record)?;
            }
        }

        let new_hwm = log.high_watermark();

        // 5. Advance follower commit index bounded by leader_commit and local_hwm (INV-4)
        let effective_commit = new_hwm.min(req.leader_commit);
        let _ = self.advance_commit_index(&req.topic, req.partition, effective_commit);

        Ok(QuorumAppendResponse {
            term: current_term,
            node_id: local_node,
            topic: req.topic,
            partition: req.partition,
            success: true,
            match_offset: new_hwm,
            error: None,
        })
    }

    /// Handles follower receipt of a `QuorumHeartbeatRequest`.
    pub fn handle_quorum_heartbeat(
        &self,
        req: QuorumHeartbeatRequest,
        broker: &BrokerServer,
    ) -> Result<QuorumHeartbeatResponse, BrokerError> {
        let local_node = self.local_node_id().to_string();
        let mut state = self.state.write().unwrap();

        if req.term < state.current_term {
            return Ok(QuorumHeartbeatResponse {
                term: state.current_term,
                node_id: local_node,
                success: false,
                high_watermarks: HashMap::new(),
            });
        }

        if req.term > state.current_term {
            state.current_term = req.term;
            state.leader_id = Some(req.leader_id.clone());
            *self.role.write().unwrap() = ReplicaRole::Follower;
            let _ = state.save_to_dir(&self.data_dir);
        }

        let mut hwms = HashMap::new();
        for (key, leader_commit) in req.commit_indices {
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() == 2
                && let Ok(partition) = parts[1].parse::<u32>()
            {
                let topic = parts[0];
                if let Some(log) = broker.get_log(topic, partition) {
                    let hwm = log.high_watermark();
                    hwms.insert(key.clone(), hwm);
                    let effective = hwm.min(leader_commit);
                    state.advance_commit_index(topic, partition, effective);
                }
            }
        }
        let _ = state.save_to_dir(&self.data_dir);

        Ok(QuorumHeartbeatResponse {
            term: state.current_term,
            node_id: local_node,
            success: true,
            high_watermarks: hwms,
        })
    }

    /// Handles vote requests during in-region candidate elections.
    pub fn handle_quorum_vote(
        &self,
        req: QuorumVoteRequest,
        _broker: &BrokerServer,
    ) -> Result<QuorumVoteResponse, BrokerError> {
        let local_node = self.local_node_id().to_string();
        let mut state = self.state.write().unwrap();

        if req.term < state.current_term {
            return Ok(QuorumVoteResponse {
                term: state.current_term,
                node_id: local_node,
                vote_granted: false,
                reason: Some(format!(
                    "Candidate term {} < local term {}",
                    req.term, state.current_term
                )),
            });
        }

        if req.term > state.current_term {
            state.current_term = req.term;
            state.voted_for = None;
            state.leader_id = None;
            *self.role.write().unwrap() = ReplicaRole::Follower;
        }

        let can_vote = match state.voted_for {
            None => true,
            Some(ref voted) => voted == &req.candidate_id,
        };

        if can_vote {
            state.voted_for = Some(req.candidate_id.clone());
            let _ = state.save_to_dir(&self.data_dir);
            Ok(QuorumVoteResponse {
                term: state.current_term,
                node_id: local_node,
                vote_granted: true,
                reason: None,
            })
        } else {
            Ok(QuorumVoteResponse {
                term: state.current_term,
                node_id: local_node,
                vote_granted: false,
                reason: Some("Already voted for a different candidate in this term".into()),
            })
        }
    }

    /// Synchronously replicates an appended batch to voting peer replicas and awaits majority quorum.
    pub async fn replicate_to_quorum(
        &self,
        topic: &str,
        partition: u32,
        epoch: u64,
        records: Vec<RecordEnvelope>,
        target_offset: u64,
        timeout_duration: Duration,
    ) -> Result<u64, BrokerError> {
        self.quorum_appends_total.fetch_add(1, Ordering::Relaxed);

        let cfg = match self.config {
            Some(ref c) => c,
            None => {
                self.quorum_appends_failed.fetch_add(1, Ordering::Relaxed);
                return Err(BrokerError::Ownership(OwnershipError::QuorumUnavailable(
                    "InRegionQuorum durability requested but no regional replica group is configured".into(),
                )));
            }
        };

        let quorum_needed = cfg.quorum_size();
        if quorum_needed <= 1 {
            let _ = self.advance_commit_index(topic, partition, target_offset);
            return Ok(target_offset);
        }

        let current_term = self.current_term();
        let leader_id = cfg.local_node_id.clone();
        let base_offset = records.first().map(|r| r.base_offset).unwrap_or(0);
        let leader_commit = self.get_commit_index(topic, partition);

        let append_req = QuorumAppendRequest {
            term: current_term,
            leader_id: leader_id.clone(),
            topic: topic.to_string(),
            partition,
            epoch,
            base_offset,
            records,
            leader_commit,
        };

        let voting_peers: Vec<(String, String)> = cfg
            .peers
            .iter()
            .filter(|p| p.is_voter)
            .map(|p| (p.node_id.clone(), p.address.clone()))
            .collect();

        let mut dispatch_handles = Vec::new();
        for (peer_node_id, peer_addr) in voting_peers {
            let req_clone = append_req.clone();
            let handle = tokio::spawn(async move {
                let connect_res =
                    timeout(timeout_duration, BrokerClient::connect(&peer_addr)).await;
                let mut client = match connect_res {
                    Ok(Ok(c)) => c,
                    Ok(Err(e)) => {
                        return (peer_node_id, false, 0, Some(format!("Connect error: {e}")));
                    }
                    Err(_) => return (peer_node_id, false, 0, Some("Connect timeout".into())),
                };

                let send_res = timeout(
                    timeout_duration,
                    client.send_request(BrokerRequest::QuorumAppend(req_clone)),
                )
                .await;

                match send_res {
                    Ok(Ok(BrokerResponse::QuorumAppend(resp))) => {
                        (peer_node_id, resp.success, resp.match_offset, resp.error)
                    }
                    Ok(Ok(other)) => (
                        peer_node_id,
                        false,
                        0,
                        Some(format!("Unexpected response: {other:?}")),
                    ),
                    Ok(Err(e)) => (peer_node_id, false, 0, Some(format!("RPC error: {e}"))),
                    Err(_) => (peer_node_id, false, 0, Some("Append RPC timeout".into())),
                }
            });
            dispatch_handles.push(handle);
        }

        let mut persisted_count = 1;
        let now_ms = Utc::now().timestamp_millis();

        for handle in dispatch_handles {
            if let Ok((peer_node_id, success, match_offset, err)) = handle.await {
                let mut telemetry = self.peers_telemetry.write().unwrap();
                if let Some(peer_stat) = telemetry.get_mut(&peer_node_id) {
                    peer_stat.connected = err.is_none();
                    if success {
                        peer_stat.last_ack_ms = now_ms;
                        peer_stat.match_offset = match_offset;
                    }
                }

                if success && match_offset >= target_offset {
                    persisted_count += 1;
                } else if let Some(e) = err {
                    tracing::warn!(peer = %peer_node_id, error = %e, "Peer rejected quorum append");
                }
            }
        }

        if persisted_count >= quorum_needed {
            let _ = self.advance_commit_index(topic, partition, target_offset);
            tracing::debug!(
                topic = %topic,
                partition = partition,
                target_offset = target_offset,
                persisted = persisted_count,
                needed = quorum_needed,
                "InRegionQuorum durability confirmed by majority replicas"
            );
            Ok(target_offset)
        } else {
            self.quorum_appends_failed.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                topic = %topic,
                partition = partition,
                persisted = persisted_count,
                needed = quorum_needed,
                "InRegionQuorum failed: required replicas did not acknowledge within timeout"
            );
            Err(BrokerError::Ownership(OwnershipError::QuorumNotReached {
                persisted: persisted_count,
            }))
        }
    }

    /// Synchronously replicates an appended batch to voting peer replicas and blocks until majority quorum.
    pub fn replicate_to_quorum_sync(
        &self,
        topic: &str,
        partition: u32,
        epoch: u64,
        records: Vec<RecordEnvelope>,
        target_offset: u64,
        timeout_duration: Duration,
    ) -> Result<u64, BrokerError> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| {
                handle.block_on(self.replicate_to_quorum(
                    topic,
                    partition,
                    epoch,
                    records,
                    target_offset,
                    timeout_duration,
                ))
            })
        } else {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| BrokerError::Io(std::io::Error::other(e.to_string())))?;
            rt.block_on(self.replicate_to_quorum(
                topic,
                partition,
                epoch,
                records,
                target_offset,
                timeout_duration,
            ))
        }
    }

    /// Assembles comprehensive quorum operational status for administrative reporting.
    pub fn status(&self, _broker: &BrokerServer) -> QuorumStatusResponse {
        let (group_id, region, local_node, total_voters, quorum_size) = match self.config {
            Some(ref c) => (
                Some(c.group_id.clone()),
                c.region.clone(),
                c.local_node_id.clone(),
                c.total_voters(),
                c.quorum_size(),
            ),
            None => (None, "standalone".into(), "standalone-node".into(), 1, 1),
        };

        let (current_term, leader_id, commit_indices) = {
            let state = self.state.read().unwrap();
            (
                state.current_term,
                state.leader_id.clone(),
                state.commit_indices.clone(),
            )
        };

        let peers: Vec<PeerReplicaStatus> = {
            let tel = self.peers_telemetry.read().unwrap();
            tel.values().cloned().collect()
        };

        QuorumStatusResponse {
            configured: self.config.is_some(),
            group_id,
            region,
            local_node_id: local_node,
            role: self.role(),
            current_term,
            leader_id,
            quorum_size,
            total_voters,
            ack_level: self.default_ack_level,
            commit_indices,
            peers,
        }
    }
}

/// Spawns a background task running periodic in-region heartbeats to keep followers aligned.
pub fn spawn_quorum_heartbeat_worker(
    manager: Arc<QuorumReplicationManager>,
    _broker: Arc<BrokerServer>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Option<tokio::task::JoinHandle<()>> {
    let cfg = manager.config.clone()?;
    let interval_ms = cfg.heartbeat_interval_ms;
    let local_node_id = cfg.local_node_id.clone();
    let peers = cfg.peers.clone();

    Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if manager.role() != ReplicaRole::Leader {
                        continue;
                    }
                    let term = manager.current_term();
                    let commit_indices = manager.state.read().unwrap().commit_indices.clone();

                    let heartbeat_req = QuorumHeartbeatRequest {
                        term,
                        leader_id: local_node_id.clone(),
                        commit_indices,
                    };

                    for peer in &peers {
                        let peer_id = peer.node_id.clone();
                        let peer_addr = peer.address.clone();
                        let req_clone = heartbeat_req.clone();
                        let mgr = manager.clone();

                        tokio::spawn(async move {
                            if let Ok(Ok(mut client)) = timeout(Duration::from_millis(500), BrokerClient::connect(&peer_addr)).await
                                && let Ok(Ok(BrokerResponse::QuorumHeartbeat(resp))) = timeout(
                                    Duration::from_millis(500),
                                    client.send_request(BrokerRequest::QuorumHeartbeat(req_clone)),
                                ).await
                            {
                                let now_ms = Utc::now().timestamp_millis();
                                let mut tel = mgr.peers_telemetry.write().unwrap();
                                if let Some(stat) = tel.get_mut(&peer_id) {
                                    stat.connected = resp.success;
                                    stat.last_ack_ms = now_ms;
                                }
                            } else {
                                let mut tel = mgr.peers_telemetry.write().unwrap();
                                if let Some(stat) = tel.get_mut(&peer_id) {
                                    stat.connected = false;
                                }
                            }
                        });
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!("Quorum heartbeat worker shutting down");
                        break;
                    }
                }
            }
        }
    }))
}
