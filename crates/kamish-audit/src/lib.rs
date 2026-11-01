//! Hash-chained, region-local decision log for Kamish.
//!
//! Complies with `project.md`:
//! - FR-13: Hash-chained, region-local decision log.
//! - §2.6: Enforcement Points Map (E1 to E10).
//! - §2.7.5: Level 3 Process 2.0 Policy Decision Sink D5.
//! - §4.7.1: Zero-payload threat mitigation — stores decisions and versions, NEVER payload.

use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;

pub mod publisher;
pub use publisher::{
    FileWitnessDestination, UnconfiguredWitnessDestination, WitnessAck, WitnessCheckpointEnvelope,
    WitnessDestination, WitnessPublisher, WitnessPublisherMetrics, spawn_witness_publisher_worker,
};

/// Predefined zero hash for the genesis entry.
pub const GENESIS_PREV_HASH: [u8; 32] = [0u8; 32];

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("Unsupported audit format {0}; legacy logs must be archived, not silently rewritten")]
    UnsupportedFormat(u32),

    #[error("Audit writer failed previously; reopen and verify the log before continuing")]
    WriterFailed,

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

    #[error(
        "Truncation detected: expected at least {expected_min} entries from trusted checkpoint, but found {found}"
    )]
    TruncationDetected { expected_min: u64, found: u64 },

    #[error("Checkpoint hash mismatch at sequence {sequence}: expected {expected}, found {found}")]
    CheckpointMismatch {
        sequence: u64,
        expected: String,
        found: String,
    },

    #[error("Missing witness checkpoint for stream {0}")]
    MissingWitnessCheckpoint(String),

    #[error("Witness signature error: {0}")]
    WitnessSignatureError(String),

    #[error("Witness authority error: {0}")]
    WitnessError(String),
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
    #[serde(default)]
    pub format_version: u32,
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
        hasher.update(b"kamish-audit-v2\0");
        hasher.update(
            serde_json::to_vec(&(
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
            ))
            .expect("audit fields contain only serializable primitive values"),
        );
        hex::encode(hasher.finalize())
    }

    /// Verifies the internal integrity of this specific entry.
    pub fn verify_self(&self) -> bool {
        if self.format_version != 2 {
            return false;
        }
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

/// Domain-separated signature payload for witness checkpoints (§5.1, W08).
#[allow(clippy::too_many_arguments)]
pub fn compute_checkpoint_signature_payload(
    stream_topic: &str,
    stream_partition: u32,
    sequence: u64,
    tail_hash: &str,
    record_count: u64,
    timestamp_ns: i64,
) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"kamish-audit-witness-checkpoint-v1\0");
    hasher.update(stream_topic.as_bytes());
    hasher.update(b":");
    hasher.update(stream_partition.to_be_bytes());
    hasher.update(b":");
    hasher.update(sequence.to_be_bytes());
    hasher.update(b":");
    hasher.update(tail_hash.as_bytes());
    hasher.update(b":");
    hasher.update(record_count.to_be_bytes());
    hasher.update(b":");
    hasher.update(timestamp_ns.to_be_bytes());
    hasher.finalize().to_vec()
}

/// Verification report providing completeness anchoring (§5.2, W08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditVerificationReport {
    pub total_entries: u64,
    pub anchored_count: u64,
    pub unanchored_entries: u64,
    pub anchored_tail_hash: String,
    pub latest_checkpoint_sequence: u64,
}

/// Checkpoint payload returned by witness quorum (§5.1, W08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditWitnessCheckpointData {
    pub sequence: u64,
    pub tail_hash: String,
    pub record_count: u64,
    pub signature_hex: String,
    pub timestamp_ns: i64,
}

/// Trait implemented by metadata consensus for independent witness anchoring (§5.1, W08).
pub trait AuditWitnessAuthority: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn commit_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
        sequence: u64,
        tail_hash: &str,
        record_count: u64,
        signature_hex: &str,
        timestamp_ns: i64,
    ) -> Result<(), AuditError>;

    fn read_latest_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
    ) -> Result<Option<AuditWitnessCheckpointData>, AuditError>;
}

impl<T: AuditWitnessAuthority + ?Sized> AuditWitnessAuthority for Arc<T> {
    #[allow(clippy::too_many_arguments)]
    fn commit_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
        sequence: u64,
        tail_hash: &str,
        record_count: u64,
        signature_hex: &str,
        timestamp_ns: i64,
    ) -> Result<(), AuditError> {
        (**self).commit_checkpoint(
            stream_topic,
            stream_partition,
            sequence,
            tail_hash,
            record_count,
            signature_hex,
            timestamp_ns,
        )
    }

    fn read_latest_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
    ) -> Result<Option<AuditWitnessCheckpointData>, AuditError> {
        (**self).read_latest_checkpoint(stream_topic, stream_partition)
    }
}

/// Independent witness service that signs and anchors audit log checkpoints (§5.1, W08).
pub struct AuditWitness {
    authority: Arc<dyn AuditWitnessAuthority>,
    topic: String,
    partition: u32,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    window_size: usize,
    witness_unreachable_count: AtomicU64,
    time_to_anchor_ms: AtomicU64,
    last_anchored_count: AtomicU64,
    last_anchored_hash: RwLock<String>,
    next_sequence: AtomicU64,
}

impl AuditWitness {
    pub fn new(
        authority: Arc<dyn AuditWitnessAuthority>,
        topic: impl Into<String>,
        partition: u32,
        signing_key: SigningKey,
        window_size: usize,
    ) -> Self {
        let verifying_key = signing_key.verifying_key();
        Self {
            authority,
            topic: topic.into(),
            partition,
            signing_key,
            verifying_key,
            window_size,
            witness_unreachable_count: AtomicU64::new(0),
            time_to_anchor_ms: AtomicU64::new(0),
            last_anchored_count: AtomicU64::new(0),
            last_anchored_hash: RwLock::new(hex::encode(GENESIS_PREV_HASH)),
            next_sequence: AtomicU64::new(0),
        }
    }

    pub fn checkpoint(&self, tail_hash: &str, count: u64) -> Result<(), AuditError> {
        let start = std::time::Instant::now();
        let seq = self.next_sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let digest = compute_checkpoint_signature_payload(
            &self.topic,
            self.partition,
            seq,
            tail_hash,
            count,
            now_ns,
        );
        let sig = self.signing_key.sign(&digest);
        let sig_hex = hex::encode(sig.to_bytes());

        match self.authority.commit_checkpoint(
            &self.topic,
            self.partition,
            seq,
            tail_hash,
            count,
            &sig_hex,
            now_ns,
        ) {
            Ok(()) => {
                self.time_to_anchor_ms
                    .store(start.elapsed().as_millis() as u64, Ordering::SeqCst);
                self.last_anchored_count.store(count, Ordering::SeqCst);
                *self.last_anchored_hash.write().unwrap() = tail_hash.to_string();
                Ok(())
            }
            Err(e) => {
                self.witness_unreachable_count
                    .fetch_add(1, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    pub fn verify_open(&self, entries: &[AuditEntry]) -> Result<(), AuditError> {
        let cp_opt = self
            .authority
            .read_latest_checkpoint(&self.topic, self.partition)?;
        if let Some(cp) = cp_opt {
            self.verify_checkpoint_integrity(&cp, entries)?;
        }
        Ok(())
    }

    pub fn verify_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<AuditVerificationReport, AuditError> {
        let entries = AuditLog::read_verified_entries(path.as_ref())?;
        let total_entries = entries.len() as u64;

        let cp_opt = self
            .authority
            .read_latest_checkpoint(&self.topic, self.partition)?;
        let cp = cp_opt.ok_or_else(|| {
            AuditError::MissingWitnessCheckpoint(format!("{}/{}", self.topic, self.partition))
        })?;

        self.verify_checkpoint_integrity(&cp, &entries)?;

        Ok(AuditVerificationReport {
            total_entries,
            anchored_count: cp.record_count,
            unanchored_entries: total_entries.saturating_sub(cp.record_count),
            anchored_tail_hash: cp.tail_hash,
            latest_checkpoint_sequence: cp.sequence,
        })
    }

    fn verify_checkpoint_integrity(
        &self,
        cp: &AuditWitnessCheckpointData,
        entries: &[AuditEntry],
    ) -> Result<(), AuditError> {
        let sig_bytes = hex::decode(&cp.signature_hex)
            .map_err(|e| AuditError::WitnessSignatureError(e.to_string()))?;
        let sig = Signature::from_slice(&sig_bytes)
            .map_err(|e| AuditError::WitnessSignatureError(e.to_string()))?;
        let digest = compute_checkpoint_signature_payload(
            &self.topic,
            self.partition,
            cp.sequence,
            &cp.tail_hash,
            cp.record_count,
            cp.timestamp_ns,
        );
        self.verifying_key
            .verify(&digest, &sig)
            .map_err(|e| AuditError::WitnessSignatureError(e.to_string()))?;

        let total = entries.len() as u64;
        if total < cp.record_count {
            return Err(AuditError::TruncationDetected {
                expected_min: cp.record_count,
                found: total,
            });
        }

        if cp.record_count > 0 {
            let idx = (cp.record_count - 1) as usize;
            if entries[idx].entry_hash != cp.tail_hash {
                return Err(AuditError::CheckpointMismatch {
                    sequence: cp.sequence,
                    expected: cp.tail_hash.clone(),
                    found: entries[idx].entry_hash.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn witness_unreachable_count(&self) -> u64 {
        self.witness_unreachable_count.load(Ordering::SeqCst)
    }

    pub fn time_to_anchor_ms(&self) -> u64 {
        self.time_to_anchor_ms.load(Ordering::SeqCst)
    }

    pub fn last_anchored_count(&self) -> u64 {
        self.last_anchored_count.load(Ordering::SeqCst)
    }

    pub fn window_size(&self) -> usize {
        self.window_size
    }
}

fn normalize_audit_path(path: &Path) -> PathBuf {
    let p = if let Ok(c) = path.canonicalize() {
        c
    } else if let (Some(parent), Some(file_name)) = (path.parent(), path.file_name()) {
        if let Ok(parent_c) = parent.canonicalize() {
            parent_c.join(file_name)
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        p
    }
}

static WITNESS_REGISTRY: Mutex<Option<HashMap<PathBuf, Arc<AuditWitness>>>> = Mutex::new(None);

struct AuditLogInner {
    writer_failed: bool,
    file_path: Option<PathBuf>,
    last_hash: String,
    next_sequence: u64,
    in_memory_entries: Vec<AuditEntry>,
    witness: Option<Arc<AuditWitness>>,
}

impl AuditLog {
    /// Creates an in-memory audit log (useful for testing or ephemeral brokers).
    pub fn new_in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuditLogInner {
                writer_failed: false,
                file_path: None,
                last_hash: hex::encode(GENESIS_PREV_HASH),
                next_sequence: 0,
                in_memory_entries: Vec::new(),
                witness: None,
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
            entries = Self::read_verified_entries(&path_buf)?;
            next_sequence = entries.len() as u64;
            if let Some(entry) = entries.last() {
                last_hash = entry.entry_hash.clone();
            }
        }

        // Verify the sink is writable before admitting traffic. Never silently
        // downgrade a configured disk audit to an in-memory log.
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path_buf)?;
        file.sync_all()?;
        #[cfg(unix)]
        File::open(
            path_buf
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )?
        .sync_all()?;
        let canonical = normalize_audit_path(&path_buf);
        let maybe_witness = {
            let guard = WITNESS_REGISTRY.lock().unwrap();
            guard.as_ref().and_then(|m| m.get(&canonical).cloned())
        };

        if let Some(ref witness) = maybe_witness {
            witness.verify_open(&entries)?;
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(AuditLogInner {
                writer_failed: false,
                file_path: Some(path_buf),
                last_hash,
                next_sequence,
                in_memory_entries: entries,
                witness: maybe_witness,
            })),
        })
    }

    /// Verify existing evidence read-only. Missing files are an error; unlike
    /// open(), this never creates, appends to, or syncs the supplied path.
    pub fn verify_file<P: AsRef<Path>>(path: P) -> Result<usize, AuditError> {
        Ok(Self::read_verified_entries(path.as_ref())?.len())
    }

    pub fn read_verified_entries(path: &Path) -> Result<Vec<AuditEntry>, AuditError> {
        let reader = BufReader::new(File::open(path)?);
        let mut entries = Vec::new();
        let mut previous = hex::encode(GENESIS_PREV_HASH);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(&line)?;
            if entry.format_version != 2 {
                return Err(AuditError::UnsupportedFormat(entry.format_version));
            }
            if entry.sequence != entries.len() as u64 {
                return Err(AuditError::SequenceOutOfOrder {
                    index: entry.sequence,
                    expected: entries.len() as u64,
                    found: entry.sequence,
                });
            }
            if entry.prev_hash != previous {
                return Err(AuditError::HashChainBroken {
                    index: entry.sequence,
                    expected: previous,
                    found: entry.prev_hash,
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
                    recorded: entry.entry_hash,
                });
            }
            previous = entry.entry_hash.clone();
            entries.push(entry);
        }
        Ok(entries)
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
        if inner.writer_failed {
            return Err(AuditError::WriterFailed);
        }

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
            format_version: 2,
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
            let result = (|| -> Result<(), AuditError> {
                let mut file = OpenOptions::new().append(true).open(path)?;
                let serialized = serde_json::to_string(&entry)?;
                writeln!(file, "{}", serialized)?;
                file.sync_all()?;
                Ok(())
            })();
            if let Err(error) = result {
                inner.writer_failed = true;
                return Err(error);
            }
        }

        inner.last_hash = entry_hash;
        inner.next_sequence += 1;
        inner.in_memory_entries.push(entry.clone());

        let to_trigger = if let Some(ref witness) = inner.witness {
            let unanchored = inner
                .next_sequence
                .saturating_sub(witness.last_anchored_count());
            if unanchored >= witness.window_size() as u64 {
                Some((
                    witness.clone(),
                    inner.last_hash.clone(),
                    inner.next_sequence,
                ))
            } else {
                None
            }
        } else {
            None
        };

        if let Some((witness, hash, count)) = to_trigger {
            let _ = witness.checkpoint(&hash, count);
        }

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

    /// Registers a witness for a given file path.
    pub fn register_witness<P: AsRef<Path>>(path: P, witness: Arc<AuditWitness>) {
        let mut guard = WITNESS_REGISTRY.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        let canonical = normalize_audit_path(path.as_ref());
        map.insert(canonical, witness);
    }

    /// Unregisters the witness for a given file path.
    pub fn unregister_witness<P: AsRef<Path>>(path: P) {
        let mut guard = WITNESS_REGISTRY.lock().unwrap();
        if let Some(map) = guard.as_mut() {
            let canonical = normalize_audit_path(path.as_ref());
            map.remove(&canonical);
        }
    }

    /// Opens an audit log with an explicit witness.
    pub fn open_with_witness<P: AsRef<Path>>(
        path: P,
        witness: Arc<AuditWitness>,
    ) -> Result<Self, AuditError> {
        Self::register_witness(&path, witness);
        Self::open(&path)
    }

    /// Returns the hash of the latest recorded entry.
    pub fn last_hash(&self) -> String {
        self.inner.lock().unwrap().last_hash.clone()
    }

    /// Returns the total number of entries recorded.
    pub fn entry_count(&self) -> u64 {
        self.inner.lock().unwrap().next_sequence
    }

    /// Attaches an independent witness to this active audit log instance.
    pub fn attach_witness(&self, witness: Arc<AuditWitness>) {
        let mut inner = self.inner.lock().unwrap();
        inner.witness = Some(witness);
    }

    /// Manually flushes and anchors the current log tail to the witness quorum.
    pub fn flush_checkpoint(&self) -> Result<(), AuditError> {
        let (witness, hash, count) = {
            let inner = self.inner.lock().unwrap();
            if let Some(ref w) = inner.witness {
                (w.clone(), inner.last_hash.clone(), inner.next_sequence)
            } else {
                return Ok(());
            }
        };
        witness.checkpoint(&hash, count)
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

    struct MockWitnessAuthority {
        checkpoint: Mutex<Option<AuditWitnessCheckpointData>>,
        fail_commits: std::sync::atomic::AtomicBool,
    }

    impl MockWitnessAuthority {
        fn new() -> Self {
            Self {
                checkpoint: Mutex::new(None),
                fail_commits: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    impl AuditWitnessAuthority for MockWitnessAuthority {
        #[allow(clippy::too_many_arguments)]
        fn commit_checkpoint(
            &self,
            _stream_topic: &str,
            _stream_partition: u32,
            sequence: u64,
            tail_hash: &str,
            record_count: u64,
            signature_hex: &str,
            timestamp_ns: i64,
        ) -> Result<(), AuditError> {
            if self.fail_commits.load(Ordering::SeqCst) {
                return Err(AuditError::WitnessError("Quorum unreachable".into()));
            }
            let mut guard = self.checkpoint.lock().unwrap();
            *guard = Some(AuditWitnessCheckpointData {
                sequence,
                tail_hash: tail_hash.to_string(),
                record_count,
                signature_hex: signature_hex.to_string(),
                timestamp_ns,
            });
            Ok(())
        }

        fn read_latest_checkpoint(
            &self,
            _stream_topic: &str,
            _stream_partition: u32,
        ) -> Result<Option<AuditWitnessCheckpointData>, AuditError> {
            Ok(self.checkpoint.lock().unwrap().clone())
        }
    }

    #[test]
    fn test_audit_witness_lifecycle_and_truncation_detection() {
        let auth = Arc::new(MockWitnessAuthority::new());
        let signing_key = SigningKey::from_bytes(&[55u8; 32]);
        let witness = Arc::new(AuditWitness::new(
            auth.clone(),
            "test-topic",
            0,
            signing_key,
            128,
        ));

        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("witnessed_audit.log");

        AuditLog::register_witness(&log_path, witness.clone());

        {
            let log = AuditLog::open(&log_path).unwrap();
            log.record(
                AuditAction::Ingress,
                "class-1",
                "reg-1",
                "reg-1",
                1,
                DecisionOutcome::AllowCleartext,
                "entry_0",
            )
            .unwrap();
            log.record(
                AuditAction::BrokerAppend,
                "class-1",
                "reg-1",
                "reg-1",
                1,
                DecisionOutcome::AllowCleartext,
                "entry_1",
            )
            .unwrap();

            // Anchor checkpoint at 2 records
            witness.checkpoint(&log.last_hash(), 2).unwrap();
            assert_eq!(witness.last_anchored_count(), 2);
        }

        // Positive control: file reopens successfully with 2 records
        assert!(AuditLog::open(&log_path).is_ok());

        // Negative control: truncation detected if line 2 is removed
        let content = std::fs::read_to_string(&log_path).unwrap();
        let first_line = content.lines().next().unwrap();
        std::fs::write(
            &log_path,
            format!(
                "{}
",
                first_line
            ),
        )
        .unwrap();

        // Reopening with registered witness MUST fail with TruncationDetected
        let reopen_res = AuditLog::open(&log_path);
        assert!(matches!(
            reopen_res,
            Err(AuditError::TruncationDetected {
                expected_min: 2,
                found: 1
            })
        ));

        // Metric test: fail commit increments unreachable count
        auth.fail_commits.store(true, Ordering::SeqCst);
        let commit_err = witness.checkpoint("some_hash", 3);
        assert!(commit_err.is_err());
        assert_eq!(witness.witness_unreachable_count(), 1);

        AuditLog::unregister_witness(&log_path);
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
