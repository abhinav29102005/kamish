//! Segmented Append-Only Log with Dense Monotonic Offsets, fsync Flush Durability, and Checksum Verification.
//!
//! Complies with `project.md`:
//! - §3.1.1: Produce Path with Policy Enforcement and Acknowledgment (fsync flush durability).
//! - §3.5.2: Record Envelope Layout (magic, epoch, class_id, policy_version, base_offset, payload, checksum).
//! - INV-1: Offsets are dense and monotonic per partition.
//! - INV-2: Acknowledged record is durable on disk before Ack.
//! - INV-4: A fetch never returns records beyond the committed high-water mark.

use crc32fast::Hasher as CrcHasher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thiserror::Error;

pub const RECORD_MAGIC_V1: u16 = 0x4859; // "HY"
pub const RECORD_MAGIC_V2: u16 = 0x485A;

/// Persisted with the payload in one checksummed/fsynced record, never in a
/// separate session file that could get ahead of or behind the commit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordContext {
    pub producer_id: u64,
    pub producer_epoch: u64,
    pub sequence: u32,
    pub origin_region: String,
    pub request_policy: String,
    /// Snapshot of permitted destinations at admission. Recovery intersects
    /// this list with current policy; it cannot invent new destinations.
    pub replication_targets: Vec<String>,
}

#[derive(Debug, Error)]
pub enum LogError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid record magic byte: {0:#x}")]
    InvalidMagic(u16),

    #[error("Checksum mismatch at offset {offset}: computed {computed:#x}, recorded {recorded:#x}")]
    ChecksumMismatch {
        offset: u64,
        computed: u32,
        recorded: u32,
    },

    #[error("Stale epoch write attempt: broker epoch is {current}, append request was {requested}")]
    StaleEpoch { current: u64, requested: u64 },

    #[error("Offset gap detected: expected monotonic offset {expected}, but received {received}")]
    NonMonotonicOffset { expected: u64, received: u64 },
}

/// Standard Hydra Stream record envelope (§3.5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordEnvelope {
    pub magic: u16,
    pub epoch: u64,
    pub class_id: String,
    pub policy_version: u64,
    pub base_offset: u64,
    pub payload: Vec<u8>,
    pub checksum: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<RecordContext>,
}

impl RecordEnvelope {
    pub fn new(
        epoch: u64,
        class_id: impl Into<String>,
        policy_version: u64,
        base_offset: u64,
        payload: Vec<u8>,
    ) -> Self {
        let class_str = class_id.into();
        let checksum =
            Self::compute_checksum(epoch, &class_str, policy_version, base_offset, &payload);
        Self {
            magic: RECORD_MAGIC_V1,
            epoch,
            class_id: class_str,
            policy_version,
            base_offset,
            payload,
            checksum,
            context: None,
        }
    }

    pub fn compute_checksum(
        epoch: u64,
        class_id: &str,
        policy_version: u64,
        base_offset: u64,
        payload: &[u8],
    ) -> u32 {
        let mut hasher = CrcHasher::new();
        hasher.update(&RECORD_MAGIC_V1.to_be_bytes());
        hasher.update(&epoch.to_be_bytes());
        hasher.update(class_id.as_bytes());
        hasher.update(&policy_version.to_be_bytes());
        hasher.update(&base_offset.to_be_bytes());
        hasher.update(payload);
        hasher.finalize()
    }

    pub fn verify_checksum(&self) -> bool {
        self.expected_checksum()
            .is_some_and(|computed| computed == self.checksum)
    }

    fn expected_checksum(&self) -> Option<u32> {
        match (self.magic, &self.context) {
            (RECORD_MAGIC_V1, None) => Some(Self::compute_checksum(
                self.epoch,
                &self.class_id,
                self.policy_version,
                self.base_offset,
                &self.payload,
            )),
            (RECORD_MAGIC_V2, Some(context)) => {
                let bytes = serde_json::to_vec(&(
                    self.magic,
                    self.epoch,
                    &self.class_id,
                    self.policy_version,
                    self.base_offset,
                    &self.payload,
                    context,
                ))
                .ok()?;
                Some(crc32fast::hash(&bytes))
            }
            _ => None,
        }
    }
}

/// In-memory and persistent partition log manager enforcing INV-1, INV-2, and INV-4.
#[derive(Clone)]
pub struct PartitionLog {
    inner: Arc<RwLock<PartitionLogInner>>,
    writer_lock: Arc<std::sync::Mutex<Option<File>>>,
}

impl std::fmt::Debug for PartitionLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.read().unwrap();
        f.debug_struct("PartitionLog")
            .field("dir", &inner.dir)
            .field("current_epoch", &inner.current_epoch)
            .field("next_offset", &inner.next_offset)
            .field("high_watermark", &inner.high_watermark)
            .finish()
    }
}

struct PartitionLogInner {
    dir: PathBuf,
    current_epoch: u64,
    next_offset: u64,
    high_watermark: u64,
    records: Vec<RecordEnvelope>,
}

impl PartitionLog {
    pub fn open<P: AsRef<Path>>(dir: P, initial_epoch: u64) -> Result<Self, LogError> {
        let dir_buf = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir_buf)?;

        let log_file = dir_buf.join("partition.data");
        let mut records = Vec::new();
        let mut next_offset = 0;

        if log_file.exists() {
            let mut file = File::open(&log_file)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;

            let total_len = buf.len() as u64;
            let mut cursor = std::io::Cursor::new(buf);
            let mut valid_file_len = 0u64;

            loop {
                let start_pos = cursor.position();
                if start_pos == total_len {
                    // Clean end of file reached
                    break;
                }

                match bincode_deserialize_envelope(&mut cursor) {
                    Ok(envelope) => {
                        let computed = envelope
                            .expected_checksum()
                            .ok_or(LogError::InvalidMagic(envelope.magic))?;
                        if computed != envelope.checksum {
                            return Err(LogError::ChecksumMismatch {
                                offset: envelope.base_offset,
                                computed,
                                recorded: envelope.checksum,
                            });
                        }
                        if envelope.base_offset != next_offset {
                            return Err(LogError::NonMonotonicOffset {
                                expected: next_offset,
                                received: envelope.base_offset,
                            });
                        }
                        next_offset += 1;
                        records.push(envelope);
                        valid_file_len = cursor.position();
                    }
                    Err(e) => {
                        // Check if this error is due to an incomplete/torn write at the very tail of the file
                        let current_pos = cursor.position();
                        let is_eof_error = e.kind() == std::io::ErrorKind::UnexpectedEof;

                        if is_eof_error && (current_pos == total_len || start_pos + 4 > total_len) {
                            // Truncate trailing torn write at segment tail back to last verified record
                            drop(file);
                            let trunc_file = OpenOptions::new().write(true).open(&log_file)?;
                            trunc_file.set_len(valid_file_len)?;
                            trunc_file.sync_all()?;
                            break;
                        } else {
                            // Middle-of-segment corruption or corrupt frame: must fail loudly!
                            return Err(LogError::Io(e));
                        }
                    }
                }
            }
        }

        let high_watermark = records.len() as u64;
        let recovered_epoch = records
            .iter()
            .map(|record| record.epoch)
            .max()
            .unwrap_or(initial_epoch);

        Ok(Self {
            inner: Arc::new(RwLock::new(PartitionLogInner {
                dir: dir_buf,
                current_epoch: initial_epoch.max(recovered_epoch),
                next_offset,
                high_watermark,
                records,
            })),
            writer_lock: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Appends a payload batch, enforcing INV-1 (monotonic dense offsets), epoch fencing (§3.4.2),
    /// and fsync flush durability to underlying disk hardware (§3.1.1, INV-2).
    ///
    /// Decoupled concurrency architecture:
    /// Disk file I/O and hardware `sync_all` are serialized under `writer_lock` without holding
    /// `inner.write()`. This guarantees concurrent readers fetching records are NEVER blocked by
    /// ongoing fsync operations.
    pub fn append(
        &self,
        epoch: u64,
        class_id: impl Into<String>,
        policy_version: u64,
        payload: Vec<u8>,
    ) -> Result<u64, LogError> {
        self.append_with_context(epoch, class_id, policy_version, payload, None)
    }

    pub fn append_with_context(
        &self,
        epoch: u64,
        class_id: impl Into<String>,
        policy_version: u64,
        payload: Vec<u8>,
        context: Option<RecordContext>,
    ) -> Result<u64, LogError> {
        // 1. Serialize writers to guarantee continuous monotonic sequence numbering (INV-1)
        let mut file_guard = self.writer_lock.lock().unwrap();

        // 2. Validate epoch and reserve next offset under brief write lock
        let (offset, dir) = {
            let mut inner = self.inner.write().unwrap();
            if epoch < inner.current_epoch {
                return Err(LogError::StaleEpoch {
                    current: inner.current_epoch,
                    requested: epoch,
                });
            }
            if epoch > inner.current_epoch {
                inner.current_epoch = epoch;
            }
            (inner.next_offset, inner.dir.clone())
        };

        let mut envelope = RecordEnvelope::new(epoch, class_id, policy_version, offset, payload);
        if context.is_some() {
            envelope.magic = RECORD_MAGIC_V2;
            envelope.context = context;
            envelope.checksum = envelope.expected_checksum().expect("valid v2 envelope");
        }

        // 3. Open or reuse persistent segment file
        if file_guard.is_none() {
            let log_path = dir.join("partition.data");
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)?;
            *file_guard = Some(file);
            #[cfg(unix)]
            File::open(&dir)?.sync_all()?;
        }

        let serialized = serde_json::to_vec(&envelope)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let len_bytes = (serialized.len() as u32).to_be_bytes();

        let file = file_guard.as_mut().unwrap();
        file.write_all(&len_bytes)?;
        file.write_all(&serialized)?;
        file.flush()?;

        // Genuine fsync flush to hardware storage controller before acknowledging append (INV-2).
        // Readers can read concurrently during this fsync with zero lock contention!
        file.sync_all()?;

        // 4. Once crash-durable on disk, publish record to in-memory committed log and advance HWM (INV-4)
        {
            let mut inner = self.inner.write().unwrap();
            inner.records.push(envelope);
            inner.next_offset += 1;
            inner.high_watermark += 1;
        }

        Ok(offset)
    }

    /// Fetches records starting at `start_offset`, capped strictly by the High-Water Mark (INV-4).
    pub fn read(&self, start_offset: u64, max_records: usize) -> Vec<RecordEnvelope> {
        let inner = self.inner.read().unwrap();
        if start_offset >= inner.high_watermark {
            return Vec::new();
        }

        let count = std::cmp::min(max_records, (inner.high_watermark - start_offset) as usize);

        inner.records[start_offset as usize..start_offset as usize + count].to_vec()
    }

    /// Sequential consumer reads between `[start_offset, end_offset)`.
    pub fn read_range(&self, start_offset: u64, end_offset: u64) -> Vec<RecordEnvelope> {
        let inner = self.inner.read().unwrap();
        if start_offset >= inner.high_watermark || start_offset >= end_offset {
            return Vec::new();
        }

        let effective_end = std::cmp::min(end_offset, inner.high_watermark) as usize;
        let start = start_offset as usize;
        inner.records[start..effective_end].to_vec()
    }

    pub fn next_offset(&self) -> u64 {
        self.inner.read().unwrap().next_offset
    }

    pub fn high_watermark(&self) -> u64 {
        self.inner.read().unwrap().high_watermark
    }
}

fn bincode_deserialize_envelope(
    cursor: &mut std::io::Cursor<Vec<u8>>,
) -> Result<RecordEnvelope, std::io::Error> {
    let mut len_bytes = [0u8; 4];
    cursor.read_exact(&mut len_bytes)?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monotonic_offset_and_crc_verification() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log = PartitionLog::open(temp_dir.path(), 1).unwrap();

        let o0 = log
            .append(1, "EU-class", 1, b"first payload".to_vec())
            .unwrap();
        assert_eq!(o0, 0);

        let o1 = log
            .append(1, "EU-class", 1, b"second payload".to_vec())
            .unwrap();
        assert_eq!(o1, 1);

        assert_eq!(log.next_offset(), 2);
        assert_eq!(log.high_watermark(), 2);

        // Fetch respects HWM (INV-4)
        let records = log.read(0, 10);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].base_offset, 0);
        assert_eq!(records[1].base_offset, 1);
        assert!(records[0].verify_checksum());
        assert!(records[1].verify_checksum());

        // Reopening preserves monotonic state
        let reopened = PartitionLog::open(temp_dir.path(), 1).unwrap();
        assert_eq!(reopened.next_offset(), 2);
    }

    #[test]
    fn test_stale_epoch_rejection() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log = PartitionLog::open(temp_dir.path(), 5).unwrap();

        let err = log
            .append(4, "EU-class", 1, b"stale write".to_vec())
            .unwrap_err();
        assert!(matches!(
            err,
            LogError::StaleEpoch {
                current: 5,
                requested: 4
            }
        ));
    }

    #[test]
    fn test_fsync_flush_durability_and_sequential_reads() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        // 1. Write 200 records with fsync durability
        {
            let log = PartitionLog::open(&path, 1).unwrap();
            for i in 0..200 {
                let payload = format!("durable-event-payload-{:04}", i).into_bytes();
                let offset = log.append(1, "sovereign-class", 1, payload).unwrap();
                assert_eq!(offset, i as u64);
            }
            assert_eq!(log.high_watermark(), 200);
            // PartitionLog dropped here, simulating broker shutdown/process termination
        }

        // 2. Restart recovery: reopen partition log from raw disk files
        let recovered = PartitionLog::open(&path, 1).unwrap();
        assert_eq!(recovered.high_watermark(), 200);
        assert_eq!(recovered.next_offset(), 200);

        // 3. Verify sequential consumer offset reads in chunks of 50
        let mut consumed = 0;
        let mut cur_offset = 0;
        while cur_offset < 200 {
            let chunk = recovered.read(cur_offset, 50);
            assert_eq!(chunk.len(), 50);
            for record in chunk {
                assert_eq!(record.base_offset, cur_offset);
                assert!(record.verify_checksum());
                let expected_payload =
                    format!("durable-event-payload-{:04}", cur_offset).into_bytes();
                assert_eq!(record.payload, expected_payload);
                cur_offset += 1;
                consumed += 1;
            }
        }
        assert_eq!(consumed, 200);

        // 4. Test read_range slice
        let slice = recovered.read_range(10, 25);
        assert_eq!(slice.len(), 15);
        assert_eq!(slice[0].base_offset, 10);
        assert_eq!(slice[14].base_offset, 24);
    }

    #[test]
    fn test_torn_write_tail_truncation_recovery() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();
        let data_path = path.join("partition.data");

        // 1. Write 3 valid records
        {
            let log = PartitionLog::open(&path, 1).unwrap();
            log.append(1, "sovereign", 1, b"record-0".to_vec()).unwrap();
            log.append(1, "sovereign", 1, b"record-1".to_vec()).unwrap();
            log.append(1, "sovereign", 1, b"record-2".to_vec()).unwrap();
            assert_eq!(log.high_watermark(), 3);
        }

        let valid_len = std::fs::metadata(&data_path).unwrap().len();

        // 2. Simulate dirty crash / torn write by appending 15 bytes of truncated corrupt data
        {
            let mut file = OpenOptions::new().append(true).open(&data_path).unwrap();
            // Incomplete header bytes representing a torn write
            file.write_all(&[0x00, 0x00, 0x01, 0xFF, 0xDE, 0xAD, 0xBE, 0xEF])
                .unwrap();
            file.flush().unwrap();
        }
        let corrupt_len = std::fs::metadata(&data_path).unwrap().len();
        assert!(corrupt_len > valid_len);

        // 3. Re-open log: it should detect the torn write at the tail, truncate back to valid_len,
        // and cleanly recover the 3 valid records.
        let recovered = PartitionLog::open(&path, 1).unwrap();
        assert_eq!(recovered.high_watermark(), 3);
        assert_eq!(recovered.next_offset(), 3);

        let records = recovered.read(0, 10);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].payload, b"record-0");
        assert_eq!(records[1].payload, b"record-1");
        assert_eq!(records[2].payload, b"record-2");

        let current_len = std::fs::metadata(&data_path).unwrap().len();
        assert_eq!(
            current_len, valid_len,
            "Tail truncation should restore exact valid length"
        );

        // 4. Verify that new appends succeed cleanly at offset 3
        let o3 = recovered
            .append(1, "sovereign", 1, b"record-3".to_vec())
            .unwrap();
        assert_eq!(o3, 3);
        assert_eq!(recovered.high_watermark(), 4);

        // 5. Subsequent reopen finds all 4 valid records intact
        let reopened = PartitionLog::open(&path, 1).unwrap();
        assert_eq!(reopened.high_watermark(), 4);
        assert_eq!(reopened.read(0, 10).len(), 4);
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Producer sequence exhausted; begin a new producer epoch")]
    SequenceExhausted,
    #[error("Out of order sequence: expected {expected}, received {received}")]
    OutOfOrderSequence { expected: u32, received: u32 },

    #[error("Producer epoch stale: expected >= {current}, received {received}")]
    StaleProducerEpoch { current: u64, received: u64 },
}

/// Producer session tracking to guarantee deduplication and sequence ordering (§3.4.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerSessionState {
    pub producer_id: u64,
    pub producer_epoch: u64,
    pub last_seq: u32,
    pub last_offset: u64,
}

/// In-memory thread-safe session manager per partition.
#[derive(Clone, Default)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<u64, ProducerSessionState>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates sequence ordering:
    /// - If seq == last_seq: returns Ok(Some(last_offset)) as an idempotent duplicate retry (INV-6).
    /// - If seq == last_seq + 1 (or first seq 0): returns Ok(None) to proceed with append.
    /// - Otherwise: returns Err(OutOfOrderSequence).
    pub fn check_sequence(
        &self,
        producer_id: u64,
        producer_epoch: u64,
        seq: u32,
    ) -> Result<Option<u64>, SessionError> {
        let sessions = self.sessions.read().unwrap();
        if let Some(state) = sessions.get(&producer_id) {
            if producer_epoch < state.producer_epoch {
                return Err(SessionError::StaleProducerEpoch {
                    current: state.producer_epoch,
                    received: producer_epoch,
                });
            }

            if producer_epoch > state.producer_epoch {
                return if seq == 0 {
                    Ok(None)
                } else {
                    Err(SessionError::OutOfOrderSequence {
                        expected: 0,
                        received: seq,
                    })
                };
            }

            if seq == state.last_seq {
                // Duplicate retry: return cached offset without appending (INV-6)
                return Ok(Some(state.last_offset));
            }

            let expected = state
                .last_seq
                .checked_add(1)
                .ok_or(SessionError::SequenceExhausted)?;
            if seq != expected {
                return Err(SessionError::OutOfOrderSequence {
                    expected,
                    received: seq,
                });
            }
        } else if seq != 0 {
            // First message from a session must start at sequence 0
            return Err(SessionError::OutOfOrderSequence {
                expected: 0,
                received: seq,
            });
        }

        Ok(None)
    }

    /// Updates session state after successful fsync and append.
    pub fn update(&self, producer_id: u64, producer_epoch: u64, seq: u32, offset: u64) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(
            producer_id,
            ProducerSessionState {
                producer_id,
                producer_epoch,
                last_seq: seq,
                last_offset: offset,
            },
        );
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[test]
    fn test_idempotent_session_deduplication() {
        let sm = SessionManager::new();

        // 1. Initial message seq 0
        let res = sm.check_sequence(101, 1, 0).unwrap();
        assert_eq!(res, None);
        sm.update(101, 1, 0, 42);

        // 2. Duplicate retry seq 0 -> returns cached offset 42 (INV-6)
        let dup = sm.check_sequence(101, 1, 0).unwrap();
        assert_eq!(dup, Some(42));

        // 3. Next message seq 1 -> proceeds
        let res1 = sm.check_sequence(101, 1, 1).unwrap();
        assert_eq!(res1, None);
        sm.update(101, 1, 1, 43);

        // 4. Out-of-order seq 3 (skipped seq 2) -> rejected
        let err = sm.check_sequence(101, 1, 3).unwrap_err();
        assert!(matches!(
            err,
            SessionError::OutOfOrderSequence {
                expected: 2,
                received: 3
            }
        ));
    }
}
