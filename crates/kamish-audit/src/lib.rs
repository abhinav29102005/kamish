//! Hash-chained, region-local decision log for Kamish.
//!
//! Complies with `project.md`:
//! - FR-13: Hash-chained, region-local decision log.
//! - §2.6: Enforcement Points Map (E1 to E10).
//! - §2.7.5: Level 3 Process 2.0 Policy Decision Sink D5.
//! - §4.7.1: Zero-payload threat mitigation — stores decisions and versions, NEVER payload.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Predefined zero hash for the genesis entry.
pub const GENESIS_PREV_HASH: [u8; 32] = [0u8; 32];

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Hash chain broken at index {index}: expected prev_hash {expected}, found {found}")]
    HashChainBroken {
        index: u64,
        expected: String,
        found: String,
    },

    #[error("Entry hash mismatch at index {index}: computed {computed}, recorded {recorded}")]
    EntryHashMismatch {
        index: u64,
        computed: String,
        recorded: String,
    },

    #[error("Sequence out of order at index {index}: expected {expected}, found {found}")]
    SequenceOutOfOrder {
        index: u64,
        expected: u64,
        found: u64,
    },
}

/// Operations subject to policy enforcement and auditing across all 10 enforcement points (§2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    /// E1: Ingress gateway (PERSIST_LOCAL)
    Ingress,
    /// E2: Broker append (PERSIST_LOCAL)
    BrokerAppend,
    /// E3: In-region replica sync (PERSIST_REPLICA)
    InRegionReplica,
    /// E4: Cross-region replication sender (SEND_REGION)
    ReplicationSender,
    /// E5: Cross-region replication receiver (PERSIST_REMOTE)
    ReplicationReceiver,
    /// E6: Backup and tiered export (EXPORT_BACKUP)
    BackupExport,
    /// E7: Consumer fetch service (SERVE_FETCH)
    ConsumerFetch,
    /// E8: Failover partition promotion (PROMOTE)
    Promote,
    /// E8-Alt: Telemetry collector export (EXPORT_TELEMETRY)
    TelemetryExport,
    /// E9: Admin / replay tools (ADMIN_READ, REPLAY)
    AdminReplay,
    /// E10: Relay hop intermediate transit (TRANSIT)
    RelayTransit,
}

/// Enforcement decision outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionOutcome {
    AllowCleartext,
    AllowSealed,
    Deny,
}

/// Individual immutable decision event in the audit chain.
/// Note: Contains NO message payload data, preserving sovereignty (§4.7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub sequence: u64,
    pub timestamp_ns: i64,
    pub prev_hash: String,
    pub action: AuditAction,
    pub class_id: String,
    pub source_region: String,
    pub target_region: String,
    pub policy_version: u64,
    pub outcome: DecisionOutcome,
    pub reason: String,
    pub entry_hash: String,
}

impl AuditEntry {
    /// Computes the SHA-256 hash over all entry metadata and the preceding hash.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_hash(
        sequence: u64,
        timestamp_ns: i64,
        prev_hash: &str,
        action: AuditAction,
        class_id: &str,
        source_region: &str,
        target_region: &str,
        policy_version: u64,
        outcome: DecisionOutcome,
        reason: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(sequence.to_be_bytes());
        hasher.update(timestamp_ns.to_be_bytes());
        hasher.update(prev_hash.as_bytes());
        hasher.update(format!("{:?}", action).as_bytes());
        hasher.update(class_id.as_bytes());
        hasher.update(source_region.as_bytes());
        hasher.update(target_region.as_bytes());
        hasher.update(policy_version.to_be_bytes());
        hasher.update(format!("{:?}", outcome).as_bytes());
        hasher.update(reason.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verifies the internal integrity of this specific entry.
    pub fn verify_self(&self) -> bool {
        let computed = Self::compute_hash(
            self.sequence,
            self.timestamp_ns,
            &self.prev_hash,
            self.action,
            &self.class_id,
            &self.source_region,
            &self.target_region,
            self.policy_version,
            self.outcome,
            &self.reason,
        );
        computed == self.entry_hash
    }
}

/// Thread-safe, append-only, verifiable audit logger.
#[derive(Clone)]
pub struct AuditLog {
    inner: Arc<Mutex<AuditLogInner>>,
}

struct AuditLogInner {
    file_path: Option<PathBuf>,
    last_hash: String,
    next_sequence: u64,
    in_memory_entries: Vec<AuditEntry>,
}

impl AuditLog {
    /// Creates an in-memory audit log (useful for testing or ephemeral brokers).
    pub fn new_in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuditLogInner {
                file_path: None,
                last_hash: hex::encode(GENESIS_PREV_HASH),
                next_sequence: 0,
                in_memory_entries: Vec::new(),
            })),
        }
    }

    /// Opens or creates an append-only audit file on disk.
    /// If an existing log file is present, its integrity is verified on startup.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, AuditError> {
        let path_buf = path.as_ref().to_path_buf();
        let mut last_hash = hex::encode(GENESIS_PREV_HASH);
        let mut next_sequence = 0;
        let mut entries = Vec::new();

        if path_buf.exists() {
            let file = File::open(&path_buf)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line_str = line?;
                if line_str.trim().is_empty() {
                    continue;
                }
                let entry: AuditEntry = serde_json::from_str(&line_str)?;

                // Sequence verification
                if entry.sequence != next_sequence {
                    return Err(AuditError::SequenceOutOfOrder {
                        index: entry.sequence,
                        expected: next_sequence,
                        found: entry.sequence,
                    });
                }

                // Chain link verification
                if entry.prev_hash != last_hash {
                    return Err(AuditError::HashChainBroken {
                        index: entry.sequence,
                        expected: last_hash,
                        found: entry.prev_hash,
                    });
                }

                // Internal checksum verification
                if !entry.verify_self() {
                    let computed = AuditEntry::compute_hash(
                        entry.sequence,
                        entry.timestamp_ns,
                        &entry.prev_hash,
                        entry.action,
                        &entry.class_id,
                        &entry.source_region,
                        &entry.target_region,
                        entry.policy_version,
                        entry.outcome,
                        &entry.reason,
                    );
                    return Err(AuditError::EntryHashMismatch {
                        index: entry.sequence,
                        computed,
                        recorded: entry.entry_hash,
                    });
                }

                last_hash = entry.entry_hash.clone();
                next_sequence += 1;
                entries.push(entry);
            }
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(AuditLogInner {
                file_path: Some(path_buf),
                last_hash,
                next_sequence,
                in_memory_entries: entries,
            })),
        })
    }

    /// Appends a new decision to the hash-chained audit log.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        action: AuditAction,
        class_id: impl Into<String>,
        source_region: impl Into<String>,
        target_region: impl Into<String>,
        policy_version: u64,
        outcome: DecisionOutcome,
        reason: impl Into<String>,
    ) -> Result<AuditEntry, AuditError> {
        let mut inner = self.inner.lock().unwrap();

        let sequence = inner.next_sequence;
        let timestamp_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let prev_hash = inner.last_hash.clone();
        let class_id = class_id.into();
        let source_region = source_region.into();
        let target_region = target_region.into();
        let reason = reason.into();

        let entry_hash = AuditEntry::compute_hash(
            sequence,
            timestamp_ns,
            &prev_hash,
            action,
            &class_id,
            &source_region,
            &target_region,
            policy_version,
            outcome,
            &reason,
        );

        let entry = AuditEntry {
            sequence,
            timestamp_ns,
            prev_hash,
            action,
            class_id,
            source_region,
            target_region,
            policy_version,
            outcome,
            reason,
            entry_hash: entry_hash.clone(),
        };

        // Write to disk if persistence is configured
        if let Some(ref path) = inner.file_path {
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            let serialized = serde_json::to_string(&entry)?;
            writeln!(file, "{}", serialized)?;
            file.flush()?;
        }

        inner.last_hash = entry_hash;
        inner.next_sequence += 1;
        inner.in_memory_entries.push(entry.clone());

        Ok(entry)
    }

    /// Scans the entire log from genesis to tail and verifies mathematical integrity.
    pub fn verify_integrity(&self) -> Result<usize, AuditError> {
        let inner = self.inner.lock().unwrap();
        let mut expected_prev = hex::encode(GENESIS_PREV_HASH);

        for (i, entry) in inner.in_memory_entries.iter().enumerate() {
            let seq = i as u64;
            if entry.sequence != seq {
                return Err(AuditError::SequenceOutOfOrder {
                    index: entry.sequence,
                    expected: seq,
                    found: entry.sequence,
                });
            }

            if entry.prev_hash != expected_prev {
                return Err(AuditError::HashChainBroken {
                    index: entry.sequence,
                    expected: expected_prev,
                    found: entry.prev_hash.clone(),
                });
            }

            if !entry.verify_self() {
                let computed = AuditEntry::compute_hash(
                    entry.sequence,
                    entry.timestamp_ns,
                    &entry.prev_hash,
                    entry.action,
                    &entry.class_id,
                    &entry.source_region,
                    &entry.target_region,
                    entry.policy_version,
                    entry.outcome,
                    &entry.reason,
                );
                return Err(AuditError::EntryHashMismatch {
                    index: entry.sequence,
                    computed,
                    recorded: entry.entry_hash.clone(),
                });
            }

            expected_prev = entry.entry_hash.clone();
        }

        Ok(inner.in_memory_entries.len())
    }

    /// Returns the total number of entries recorded.
    pub fn count(&self) -> usize {
        self.inner.lock().unwrap().in_memory_entries.len()
    }

    /// Returns all audit entries.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.inner.lock().unwrap().in_memory_entries.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_chain_genesis_and_progression() {
        let log = AuditLog::new_in_memory();

        let e0 = log
            .record(
                AuditAction::Ingress,
                "EU-restricted",
                "eu-central",
                "eu-central",
                1,
                DecisionOutcome::AllowCleartext,
                "ok",
            )
            .unwrap();

        assert_eq!(e0.sequence, 0);
        assert_eq!(e0.prev_hash, hex::encode(GENESIS_PREV_HASH));

        let e1 = log
            .record(
                AuditAction::ReplicationSender,
                "EU-restricted",
                "eu-central",
                "us-east",
                1,
                DecisionOutcome::Deny,
                "not_in_legal_set",
            )
            .unwrap();

        assert_eq!(e1.sequence, 1);
        assert_eq!(e1.prev_hash, e0.entry_hash);

        assert_eq!(log.verify_integrity().unwrap(), 2);
    }

    #[test]
    fn test_disk_persistence_and_reopen_verification() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("audit.log");

        {
            let log = AuditLog::open(&log_path).unwrap();
            log.record(
                AuditAction::BrokerAppend,
                "Class-A",
                "reg-1",
                "reg-1",
                10,
                DecisionOutcome::AllowCleartext,
                "ok",
            )
            .unwrap();
            log.record(
                AuditAction::ReplicationSender,
                "Class-A",
                "reg-1",
                "reg-2",
                10,
                DecisionOutcome::AllowSealed,
                "ok_sealed",
            )
            .unwrap();
        }

        // Reopen log and verify integrity
        let reopened = AuditLog::open(&log_path).unwrap();
        assert_eq!(reopened.count(), 2);
        assert_eq!(reopened.verify_integrity().unwrap(), 2);
    }

    #[test]
    fn test_tamper_detection() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("tampered_audit.log");

        {
            let log = AuditLog::open(&log_path).unwrap();
            log.record(
                AuditAction::Ingress,
                "Class-X",
                "reg-1",
                "reg-1",
                1,
                DecisionOutcome::Deny,
                "tamper_me",
            )
            .unwrap();
        }

        // Tamper with the raw log file by flipping a byte in the reason field
        let content = std::fs::read_to_string(&log_path).unwrap();
        let tampered = content.replace("tamper_me", "bypassed_check");
        std::fs::write(&log_path, tampered).unwrap();

        // Reopen must fail with EntryHashMismatch
        let result = AuditLog::open(&log_path);
        assert!(result.is_err());
    }
}
