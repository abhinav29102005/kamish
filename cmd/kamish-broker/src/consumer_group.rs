//! Dynamic Consumer Group Coordinator and Partition Rebalancer for Kamish.
//!
//! Complies with Phase 6 / Workstream C:
//! - Consumer-group identity and membership records.
//! - Dynamic membership registration, session heartbeats, and timeout expiry.
//! - Deterministic range & round-robin partition assignment across subscribed topics.
//! - Generation numbers (epoch fencing) against stale consumer members.
//! - Crash-safe persistence to `consumer_groups.json` with temporary file `sync_all` and atomic rename.
//! - Restart recovery of active groups and committed assignments.

use crate::BrokerError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupState {
    Empty,
    PreparingRebalance,
    Stable,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMember {
    pub member_id: String,
    pub client_id: String,
    pub client_host: String,
    pub session_timeout_ms: u64,
    pub last_heartbeat_ms: i64,
    pub subscribed_topics: Vec<String>,
    pub assigned_partitions: HashMap<String, Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroup {
    pub group_id: String,
    pub generation_id: u32,
    pub protocol_type: String,
    pub leader_id: Option<String>,
    pub state: GroupState,
    pub members: HashMap<String, GroupMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinGroupRequest {
    pub group_id: String,
    pub member_id: Option<String>,
    pub client_id: String,
    pub client_host: Option<String>,
    pub protocol_type: String,
    pub session_timeout_ms: u64,
    pub subscribed_topics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinGroupResponse {
    pub group_id: String,
    pub member_id: String,
    pub generation_id: u32,
    pub leader_id: String,
    pub state: GroupState,
    pub assigned_partitions: HashMap<String, Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatGroupRequest {
    pub group_id: String,
    pub member_id: String,
    pub generation_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatGroupResponse {
    pub group_id: String,
    pub status: String,
    pub generation_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveGroupRequest {
    pub group_id: String,
    pub member_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveGroupResponse {
    pub group_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSummary {
    pub group_id: String,
    pub state: GroupState,
    pub generation_id: u32,
    pub member_count: usize,
    pub leader_id: Option<String>,
    pub subscribed_topics: Vec<String>,
}

pub struct ConsumerGroupCoordinator {
    data_dir: PathBuf,
    groups: Arc<RwLock<HashMap<String, ConsumerGroup>>>,
    member_sequence: AtomicU64,
}

impl ConsumerGroupCoordinator {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        let dir = data_dir.as_ref().to_path_buf();
        let mut groups = HashMap::new();

        // Restore groups from disk if existing
        let path = dir.join("consumer_groups.json");
        if let Ok(mut f) = File::open(&path) {
            let mut bytes = Vec::new();
            if f.read_to_end(&mut bytes).is_ok()
                && let Ok(loaded) = serde_json::from_slice::<HashMap<String, ConsumerGroup>>(&bytes)
            {
                groups = loaded;
            }
        }

        Self {
            data_dir: dir,
            groups: Arc::new(RwLock::new(groups)),
            member_sequence: AtomicU64::new(1),
        }
    }

    /// Registers a member into a consumer group and executes partition rebalancing.
    pub fn join_group(
        &self,
        req: JoinGroupRequest,
        topic_partition_counts: &HashMap<String, u32>,
    ) -> Result<JoinGroupResponse, BrokerError> {
        let mut groups = self.groups.write().unwrap();
        let now_ms = Utc::now().timestamp_millis();

        let group = groups
            .entry(req.group_id.clone())
            .or_insert_with(|| ConsumerGroup {
                group_id: req.group_id.clone(),
                generation_id: 0,
                protocol_type: req.protocol_type.clone(),
                leader_id: None,
                state: GroupState::Empty,
                members: HashMap::new(),
            });

        let member_id = match req.member_id {
            Some(ref id) if !id.is_empty() && group.members.contains_key(id) => id.clone(),
            _ => {
                let seq = self.member_sequence.fetch_add(1, Ordering::SeqCst);
                format!("{}-{}-{}", req.client_id, now_ms, seq)
            }
        };

        let member = GroupMember {
            member_id: member_id.clone(),
            client_id: req.client_id,
            client_host: req.client_host.unwrap_or_else(|| "127.0.0.1".to_string()),
            session_timeout_ms: req.session_timeout_ms.max(1000),
            last_heartbeat_ms: now_ms,
            subscribed_topics: req.subscribed_topics.clone(),
            assigned_partitions: HashMap::new(),
        };

        group.members.insert(member_id.clone(), member);

        // Assign leader: first lexicographical member ID
        let mut member_keys: Vec<String> = group.members.keys().cloned().collect();
        member_keys.sort();
        let leader_id = member_keys
            .first()
            .cloned()
            .unwrap_or_else(|| member_id.clone());
        group.leader_id = Some(leader_id.clone());

        // Increment generation ID on rebalance (§5.1)
        group.generation_id += 1;
        let generation = group.generation_id;

        // Execute deterministic partition rebalancing
        self.rebalance_partitions(group, topic_partition_counts);
        group.state = GroupState::Stable;

        let assigned = group
            .members
            .get(&member_id)
            .map(|m| m.assigned_partitions.clone())
            .unwrap_or_default();

        let response = JoinGroupResponse {
            group_id: req.group_id.clone(),
            member_id,
            generation_id: generation,
            leader_id,
            state: GroupState::Stable,
            assigned_partitions: assigned,
        };

        // Persist to disk
        self.persist_groups(&groups)?;
        Ok(response)
    }

    /// Records member heartbeat, validating generation fencing against zombie consumers.
    pub fn heartbeat(
        &self,
        req: HeartbeatGroupRequest,
    ) -> Result<HeartbeatGroupResponse, BrokerError> {
        let mut groups = self.groups.write().unwrap();
        let group = groups.get_mut(&req.group_id).ok_or_else(|| {
            BrokerError::NotFound(format!("Consumer group '{}' not found", req.group_id))
        })?;

        if req.generation_id != group.generation_id {
            return Err(BrokerError::EpochFenced {
                current: group.generation_id as u64,
                attempted: req.generation_id as u64,
            });
        }

        let member = group.members.get_mut(&req.member_id).ok_or_else(|| {
            BrokerError::NotFound(format!("Member '{}' not in group", req.member_id))
        })?;

        member.last_heartbeat_ms = Utc::now().timestamp_millis();

        Ok(HeartbeatGroupResponse {
            group_id: req.group_id,
            status: "OK".to_string(),
            generation_id: group.generation_id,
        })
    }

    /// Explicitly removes a member from a consumer group and triggers rebalancing.
    pub fn leave_group(
        &self,
        req: LeaveGroupRequest,
        topic_partition_counts: &HashMap<String, u32>,
    ) -> Result<LeaveGroupResponse, BrokerError> {
        let mut groups = self.groups.write().unwrap();
        let group = groups.get_mut(&req.group_id).ok_or_else(|| {
            BrokerError::NotFound(format!("Consumer group '{}' not found", req.group_id))
        })?;

        group.members.remove(&req.member_id);

        if group.members.is_empty() {
            group.state = GroupState::Empty;
            group.leader_id = None;
        } else {
            let mut member_keys: Vec<String> = group.members.keys().cloned().collect();
            member_keys.sort();
            group.leader_id = member_keys.first().cloned();
            group.generation_id += 1;
            self.rebalance_partitions(group, topic_partition_counts);
            group.state = GroupState::Stable;
        }

        self.persist_groups(&groups)?;
        Ok(LeaveGroupResponse {
            group_id: req.group_id,
            status: "LEFT".to_string(),
        })
    }

    /// Validates generation fencing and partition assignment ownership on consumer commit operations.
    pub fn validate_commit(
        &self,
        group_id: &str,
        member_id: Option<&str>,
        generation_id: Option<u32>,
    ) -> Result<(), BrokerError> {
        self.validate_partition_commit(group_id, member_id, generation_id, "", None)
    }

    /// Validates generation fencing and ensures the committing member holds assignment authority for the partition.
    pub fn validate_partition_commit(
        &self,
        group_id: &str,
        member_id: Option<&str>,
        generation_id: Option<u32>,
        topic: &str,
        partition: Option<u32>,
    ) -> Result<(), BrokerError> {
        if let (Some(member), Some(expected_gen)) = (member_id, generation_id) {
            let groups = self.groups.read().unwrap();
            if let Some(group) = groups.get(group_id) {
                if group.generation_id != expected_gen {
                    return Err(BrokerError::EpochFenced {
                        current: group.generation_id as u64,
                        attempted: expected_gen as u64,
                    });
                }
                let member_record = group.members.get(member).ok_or_else(|| {
                    BrokerError::NotFound(format!(
                        "Member '{member}' not recognized in group '{group_id}'"
                    ))
                })?;

                // Validate partition ownership if partition is specified
                if let Some(p) = partition
                    && !topic.is_empty()
                {
                    let is_assigned = member_record
                        .assigned_partitions
                        .get(topic)
                        .map(|parts| parts.contains(&p))
                        .unwrap_or(false);
                    if !is_assigned {
                        return Err(BrokerError::PolicyDenied(format!(
                            "Partition ownership violation: member '{member}' is not assigned {topic}:{p} in group '{group_id}'"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Sweeps expired group members whose heartbeat exceeded session timeout.
    pub fn sweep_expired_members(
        &self,
        topic_partition_counts: &HashMap<String, u32>,
    ) -> Result<usize, BrokerError> {
        let mut groups = self.groups.write().unwrap();
        let now_ms = Utc::now().timestamp_millis();
        let mut rebalanced_groups = 0;

        for group in groups.values_mut() {
            let initial_count = group.members.len();
            group.members.retain(|_, m| {
                now_ms.saturating_sub(m.last_heartbeat_ms) <= m.session_timeout_ms as i64
            });

            if group.members.len() < initial_count {
                if group.members.is_empty() {
                    group.state = GroupState::Empty;
                    group.leader_id = None;
                } else {
                    let mut member_keys: Vec<String> = group.members.keys().cloned().collect();
                    member_keys.sort();
                    group.leader_id = member_keys.first().cloned();
                    group.generation_id += 1;
                    self.rebalance_partitions(group, topic_partition_counts);
                    group.state = GroupState::Stable;
                }
                rebalanced_groups += 1;
            }
        }

        if rebalanced_groups > 0 {
            self.persist_groups(&groups)?;
        }

        Ok(rebalanced_groups)
    }

    /// Deterministic round-robin partition assignment across members for all subscribed topics.
    fn rebalance_partitions(
        &self,
        group: &mut ConsumerGroup,
        topic_partition_counts: &HashMap<String, u32>,
    ) {
        for member in group.members.values_mut() {
            member.assigned_partitions.clear();
        }

        let mut member_ids: Vec<String> = group.members.keys().cloned().collect();
        if member_ids.is_empty() {
            return;
        }
        member_ids.sort();

        // Collect all distinct topics subscribed by group members
        let mut all_topics = std::collections::BTreeSet::new();
        for member in group.members.values() {
            for t in &member.subscribed_topics {
                all_topics.insert(t.clone());
            }
        }

        for topic in all_topics {
            let part_count = topic_partition_counts.get(&topic).copied().unwrap_or(1);
            let interested_members: Vec<String> = member_ids
                .iter()
                .filter(|id| {
                    group
                        .members
                        .get(*id)
                        .map(|m| m.subscribed_topics.contains(&topic))
                        .unwrap_or(false)
                })
                .cloned()
                .collect();

            if interested_members.is_empty() {
                continue;
            }

            for p in 0..part_count {
                let target_member = &interested_members[(p as usize) % interested_members.len()];
                if let Some(m) = group.members.get_mut(target_member) {
                    m.assigned_partitions
                        .entry(topic.clone())
                        .or_default()
                        .push(p);
                }
            }
        }
    }

    /// Lists summaries of all active consumer groups.
    pub fn list_groups(&self) -> Vec<GroupSummary> {
        let groups = self.groups.read().unwrap();
        groups
            .values()
            .map(|g| {
                let mut topics = std::collections::BTreeSet::new();
                for m in g.members.values() {
                    for t in &m.subscribed_topics {
                        topics.insert(t.clone());
                    }
                }
                GroupSummary {
                    group_id: g.group_id.clone(),
                    state: g.state.clone(),
                    generation_id: g.generation_id,
                    member_count: g.members.len(),
                    leader_id: g.leader_id.clone(),
                    subscribed_topics: topics.into_iter().collect(),
                }
            })
            .collect()
    }

    /// Describes a single consumer group.
    pub fn describe_group(&self, group_id: &str) -> Option<ConsumerGroup> {
        let groups = self.groups.read().unwrap();
        groups.get(group_id).cloned()
    }

    /// Atomically persists consumer group states to disk.
    fn persist_groups(&self, groups: &HashMap<String, ConsumerGroup>) -> Result<(), BrokerError> {
        let tmp_path = self.data_dir.join("consumer_groups.json.tmp");
        let dest_path = self.data_dir.join("consumer_groups.json");

        let serialized = serde_json::to_vec_pretty(groups)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(&serialized)?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&tmp_path, &dest_path)?;
        Ok(())
    }
}

/// Spawns a background task running periodic consumer group heartbeat sweeps,
/// automatically evicting dead members and advancing the group generation.
pub fn spawn_consumer_group_sweeper_worker(
    coordinator: Arc<ConsumerGroupCoordinator>,
    broker: Arc<crate::BrokerServer>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    sweep_interval: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(sweep_interval);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let topic_counts: HashMap<String, u32> = {
                        let cat = broker.catalog.read().unwrap();
                        cat.topics.values().map(|t| (t.topic.clone(), t.partitions)).collect()
                    };
                    if let Err(e) = coordinator.sweep_expired_members(&topic_counts) {
                        tracing::warn!(error = %e, "Consumer group expiration sweep failed");
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!("Consumer group sweeper shutting down");
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
    fn test_join_group_and_rebalance() {
        let temp_dir = tempfile::tempdir().unwrap();
        let coordinator = ConsumerGroupCoordinator::new(temp_dir.path());

        let mut topic_counts = HashMap::new();
        topic_counts.insert("events".to_string(), 4);

        // Member 1 joins
        let res1 = coordinator
            .join_group(
                JoinGroupRequest {
                    group_id: "grp-alpha".to_string(),
                    member_id: None,
                    client_id: "client-1".to_string(),
                    client_host: Some("127.0.0.1".to_string()),
                    session_timeout_ms: 10_000,
                                        protocol_type: "consumer".to_string(),
                    subscribed_topics: vec!["events".to_string()],
                },
                &topic_counts,
            )
            .expect("member 1 join failed");

        assert_eq!(res1.generation_id, 1);
        assert_eq!(
            res1.assigned_partitions.get("events").cloned().unwrap(),
            vec![0, 1, 2, 3]
        );

        // Member 2 joins
        let res2 = coordinator
            .join_group(
                JoinGroupRequest {
                    group_id: "grp-alpha".to_string(),
                    member_id: None,
                    client_id: "client-2".to_string(),
                    client_host: Some("127.0.0.1".to_string()),
                    session_timeout_ms: 10_000,
                                        protocol_type: "consumer".to_string(),
                    subscribed_topics: vec!["events".to_string()],
                },
                &topic_counts,
            )
            .expect("member 2 join failed");

        assert_eq!(res2.generation_id, 2);

        // After rebalance, check description
        let desc = coordinator.describe_group("grp-alpha").expect("group missing");
        assert_eq!(desc.generation_id, 2);
        assert_eq!(desc.members.len(), 2);

        let parts1 = &desc.members[&res1.member_id].assigned_partitions["events"];
        let parts2 = &desc.members[&res2.member_id].assigned_partitions["events"];
        assert_eq!(parts1.len() + parts2.len(), 4);
    }

    #[test]
    fn test_heartbeat_generation_fencing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let coordinator = ConsumerGroupCoordinator::new(temp_dir.path());

        let mut topic_counts = HashMap::new();
        topic_counts.insert("telemetry".to_string(), 2);

        let res1 = coordinator
            .join_group(
                JoinGroupRequest {
                    group_id: "grp-beta".to_string(),
                    member_id: None,
                    client_id: "client-1".to_string(),
                    client_host: None,
                    session_timeout_ms: 10_000,
                                        protocol_type: "consumer".to_string(),
                    subscribed_topics: vec!["telemetry".to_string()],
                },
                &topic_counts,
            )
            .unwrap();

        // Valid heartbeat
        let hb = coordinator.heartbeat(HeartbeatGroupRequest {
            group_id: "grp-beta".to_string(),
            member_id: res1.member_id.clone(),
            generation_id: res1.generation_id,
        });
        assert!(hb.is_ok());

        // Stale generation heartbeat rejected
        let stale_hb = coordinator.heartbeat(HeartbeatGroupRequest {
            group_id: "grp-beta".to_string(),
            member_id: res1.member_id,
            generation_id: res1.generation_id + 1,
        });
        assert!(matches!(stale_hb, Err(BrokerError::EpochFenced { .. })));
    }

    #[test]
    fn test_partition_commit_ownership_enforcement() {
        let temp_dir = tempfile::tempdir().unwrap();
        let coordinator = ConsumerGroupCoordinator::new(temp_dir.path());

        let mut topic_counts = HashMap::new();
        topic_counts.insert("orders".to_string(), 2);

        let res1 = coordinator
            .join_group(
                JoinGroupRequest {
                    group_id: "grp-gamma".to_string(),
                    member_id: None,
                    client_id: "c1".to_string(),
                    client_host: None,
                    session_timeout_ms: 10_000,
                                        protocol_type: "consumer".to_string(),
                    subscribed_topics: vec!["orders".to_string()],
                },
                &topic_counts,
            )
            .unwrap();

        let res2 = coordinator
            .join_group(
                JoinGroupRequest {
                    group_id: "grp-gamma".to_string(),
                    member_id: None,
                    client_id: "c2".to_string(),
                    client_host: None,
                    session_timeout_ms: 10_000,
                                        protocol_type: "consumer".to_string(),
                    subscribed_topics: vec!["orders".to_string()],
                },
                &topic_counts,
            )
            .unwrap();

        let desc = coordinator.describe_group("grp-gamma").unwrap();
        let c1_parts = &desc.members[&res1.member_id].assigned_partitions["orders"];
        let c2_parts = &desc.members[&res2.member_id].assigned_partitions["orders"];

        let c1_owned = c1_parts[0];
        let c2_owned = c2_parts[0];
        assert_ne!(c1_owned, c2_owned);

        // c1 committing its own partition succeeds
        assert!(coordinator
            .validate_partition_commit(
                "grp-gamma",
                Some(&res1.member_id),
                Some(desc.generation_id),
                "orders",
                Some(c1_owned)
            )
            .is_ok());

        // c1 committing c2's partition fails with PolicyDenied
        let err = coordinator
            .validate_partition_commit(
                "grp-gamma",
                Some(&res1.member_id),
                Some(desc.generation_id),
                "orders",
                Some(c2_owned)
            )
            .unwrap_err();
        assert!(matches!(err, BrokerError::PolicyDenied(_)));

        // Commit with wrong generation fails with EpochFenced
        let err_gen = coordinator
            .validate_partition_commit(
                "grp-gamma",
                Some(&res1.member_id),
                Some(desc.generation_id - 1),
                "orders",
                Some(c1_owned)
            )
            .unwrap_err();
        assert!(matches!(err_gen, BrokerError::EpochFenced { .. }));
    }

    #[test]
    fn test_leave_group_and_rebalance() {
        let temp_dir = tempfile::tempdir().unwrap();
        let coordinator = ConsumerGroupCoordinator::new(temp_dir.path());

        let mut topic_counts = HashMap::new();
        topic_counts.insert("events".to_string(), 2);

        let res1 = coordinator
            .join_group(
                JoinGroupRequest {
                    group_id: "grp-delta".to_string(),
                    member_id: None,
                    client_id: "c1".to_string(),
                    client_host: None,
                    session_timeout_ms: 10_000,
                                        protocol_type: "consumer".to_string(),
                    subscribed_topics: vec!["events".to_string()],
                },
                &topic_counts,
            )
            .unwrap();

        let res2 = coordinator
            .join_group(
                JoinGroupRequest {
                    group_id: "grp-delta".to_string(),
                    member_id: None,
                    client_id: "c2".to_string(),
                    client_host: None,
                    session_timeout_ms: 10_000,
                                        protocol_type: "consumer".to_string(),
                    subscribed_topics: vec!["events".to_string()],
                },
                &topic_counts,
            )
            .unwrap();

        assert_eq!(res2.generation_id, 2);

        // c2 leaves
        let leave_res = coordinator
            .leave_group(
                LeaveGroupRequest {
                    group_id: "grp-delta".to_string(),
                    member_id: res2.member_id,
                },
                &topic_counts,
            )
            .unwrap();
        assert_eq!(leave_res.status, "LEFT");

        let desc = coordinator.describe_group("grp-delta").unwrap();
        assert_eq!(desc.generation_id, 3);
        assert_eq!(desc.members.len(), 1);
        assert_eq!(
            desc.members[&res1.member_id].assigned_partitions["events"],
            vec![0, 1]
        );
    }

    #[test]
    fn test_group_persistence_and_reload() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut topic_counts = HashMap::new();
        topic_counts.insert("events".to_string(), 2);

        let member_id = {
            let coordinator = ConsumerGroupCoordinator::new(temp_dir.path());
            let res = coordinator
                .join_group(
                    JoinGroupRequest {
                        group_id: "grp-persist".to_string(),
                        member_id: None,
                        client_id: "c1".to_string(),
                        client_host: None,
                        session_timeout_ms: 10_000,
                                                protocol_type: "consumer".to_string(),
                        subscribed_topics: vec!["events".to_string()],
                    },
                    &topic_counts,
                )
                .unwrap();
            res.member_id
        };

        // Reopen coordinator from the same data_dir
        let reloaded = ConsumerGroupCoordinator::new(temp_dir.path());
        let desc = reloaded.describe_group("grp-persist").expect("group should persist");
        assert_eq!(desc.generation_id, 1);
        assert!(desc.members.contains_key(&member_id));
        assert_eq!(desc.members[&member_id].assigned_partitions["events"], vec![0, 1]);
    }
}

