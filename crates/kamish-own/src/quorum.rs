//! Persistent Regional Replica Membership, Quorum Configuration, and Consensus State.
//!
//! Complies with Phase 2 / In-Region Quorum Consensus Architecture:
//! - Stable replica identity (`node_id`, `region`, `address`).
//! - Explicit, persisted replica group membership with strict voter counts.
//! - Majority quorum definition: M = floor(V / 2) + 1.
//! - Crash-safe atomic persistence of terms, votes, and commit indices to `consensus_state.json`.
//! - Interrupted write recovery for temporary consensus files.
//! - Promotion frontier calculation ensuring new leaders hold all quorum-committed records.

use crate::OwnershipError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Role of an in-region partition replica in consensus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReplicaRole {
    #[default]
    Follower,
    Candidate,
    Leader,
}

/// Configuration for a remote peer in the regional replica group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaPeerConfig {
    pub node_id: String,
    pub address: String,
    #[serde(default = "default_true")]
    pub is_voter: bool,
}

fn default_true() -> bool {
    true
}

fn default_election_timeout_ms() -> u64 {
    2000
}

fn default_heartbeat_interval_ms() -> u64 {
    300
}

/// Persistent regional replica group configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaGroupConfig {
    pub group_id: String,
    pub region: String,
    pub local_node_id: String,
    pub peers: Vec<ReplicaPeerConfig>,
    #[serde(default = "default_election_timeout_ms")]
    pub election_timeout_ms: u64,
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,
}

impl ReplicaGroupConfig {
    /// Total number of voting nodes in the regional group (including local node).
    pub fn total_voters(&self) -> usize {
        // Local node is always a voting member of its group
        1 + self.peers.iter().filter(|p| p.is_voter).count()
    }

    /// Strict majority quorum size: M = floor(V / 2) + 1.
    /// For a 3-node voter group: floor(3 / 2) + 1 = 2.
    /// For a 1-node group (standalone): floor(1 / 2) + 1 = 1.
    pub fn quorum_size(&self) -> usize {
        let voters = self.total_voters();
        (voters / 2) + 1
    }

    /// Validates the group configuration against membership invariants.
    pub fn validate(&self) -> Result<(), OwnershipError> {
        if self.group_id.trim().is_empty() {
            return Err(OwnershipError::MetaConflict(
                "Replica group_id cannot be empty".into(),
            ));
        }
        if self.local_node_id.trim().is_empty() {
            return Err(OwnershipError::MetaConflict(
                "Replica local_node_id cannot be empty".into(),
            ));
        }

        let mut seen_ids = std::collections::HashSet::new();
        seen_ids.insert(self.local_node_id.clone());

        for peer in &self.peers {
            if peer.node_id.trim().is_empty() {
                return Err(OwnershipError::MetaConflict(
                    "Replica peer node_id cannot be empty".into(),
                ));
            }
            if !seen_ids.insert(peer.node_id.clone()) {
                return Err(OwnershipError::MetaConflict(format!(
                    "Duplicate replica node_id '{}' detected in replica group configuration",
                    peer.node_id
                )));
            }
            if peer.address.trim().is_empty() {
                return Err(OwnershipError::MetaConflict(format!(
                    "Peer '{}' has an empty network address",
                    peer.node_id
                )));
            }
        }

        if self.total_voters() < 1 {
            return Err(OwnershipError::MetaConflict(
                "Replica group must have at least 1 voting member".into(),
            ));
        }

        Ok(())
    }

    /// Returns whether the specified peer is configured as a voting member.
    pub fn is_voting_peer(&self, node_id: &str) -> bool {
        if node_id == self.local_node_id {
            return true;
        }
        self.peers
            .iter()
            .find(|p| p.node_id == node_id)
            .map(|p| p.is_voter)
            .unwrap_or(false)
    }

    /// Returns peer address by node ID if registered.
    pub fn peer_address(&self, node_id: &str) -> Option<&str> {
        self.peers
            .iter()
            .find(|p| p.node_id == node_id)
            .map(|p| p.address.as_str())
    }
}

/// Persistent consensus state stored on disk surviving process restarts.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PersistentConsensusState {
    pub current_term: u64,
    pub leader_id: Option<String>,
    pub voted_for: Option<String>,
    pub commit_indices: HashMap<String, u64>,
    pub membership_generation: u64,
}

impl PersistentConsensusState {
    /// Formats a partition key string for commit index lookups.
    pub fn partition_key(topic: &str, partition: u32) -> String {
        format!("{topic}:{partition}")
    }

    /// Retrieves committed index for a partition.
    pub fn get_commit_index(&self, topic: &str, partition: u32) -> u64 {
        let key = Self::partition_key(topic, partition);
        self.commit_indices.get(&key).copied().unwrap_or(0)
    }

    /// Updates committed index for a partition monotonically.
    pub fn advance_commit_index(&mut self, topic: &str, partition: u32, index: u64) -> bool {
        let key = Self::partition_key(topic, partition);
        let current = self.commit_indices.entry(key).or_insert(0);
        if index > *current {
            *current = index;
            true
        } else {
            false
        }
    }

    /// Loads consensus state from the specified data directory.
    pub fn load_from_dir(dir: &Path) -> Result<Self, std::io::Error> {
        let main_file = dir.join("consensus_state.json");
        let tmp_file = dir.join("consensus_state.json.tmp");

        // 1. Crash recovery: if main file is missing/empty, try to recover from tmp_file
        let main_empty_or_missing = !main_file.exists()
            || std::fs::metadata(&main_file)
                .map(|m| m.len() == 0)
                .unwrap_or(true);

        if main_empty_or_missing && tmp_file.exists() {
            if let Ok(content) = std::fs::read(&tmp_file)
                && let Ok(state) = serde_json::from_slice::<Self>(&content)
            {
                let _ = std::fs::rename(&tmp_file, &main_file);
                return Ok(state);
            }
        }

        // 2. Normal read from main file
        if main_file.exists() {
            let mut file = File::open(&main_file)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            if !bytes.is_empty() {
                let state: Self = serde_json::from_slice(&bytes).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Corrupt consensus_state.json: {e}"),
                    )
                })?;
                return Ok(state);
            }
        }

        Ok(Self::default())
    }

    /// Persists consensus state atomically to disk using temporary file, sync_all, and atomic rename.
    pub fn save_to_dir(&self, dir: &Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let main_file = dir.join("consensus_state.json");
        let tmp_file = dir.join("consensus_state.json.tmp");

        let serialized = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_file)?;

        file.write_all(&serialized)?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&tmp_file, &main_file)?;
        Ok(())
    }
}

/// Verifies quorum frontier against surviving replicas:
/// Candidate must hold high watermark >= median watermark of majority quorum.
pub fn calculate_quorum_frontier(watermarks: &[u64], quorum_size: usize) -> u64 {
    if watermarks.is_empty() {
        return 0;
    }
    let mut sorted = watermarks.to_vec();
    sorted.sort_unstable();

    // The quorum frontier is the N - quorum_size entry from the highest.
    // For 3 replicas and quorum 2: sorted[3 - 2] = sorted[1] (2nd highest watermark).
    // For 5 replicas and quorum 3: sorted[5 - 3] = sorted[2] (3rd highest watermark).
    let idx = sorted.len().saturating_sub(quorum_size);
    sorted[idx]
}
