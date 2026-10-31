//! Segmented Append-Only Log with Dense Monotonic Offsets and Envelope Checksums.
//!
//! Complies with `project.md`:
//! - §3.5.2: Record Envelope Layout (magic, epoch, class_id, policy_version, base_offset, payload, checksum).
//! - INV-1: Offsets are dense and monotonic per partition.
//! - INV-4: A fetch never returns records beyond the committed high-water mark.

use crc32fast::Hasher as CrcHasher;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thiserror::Error;

pub const RECORD_MAGIC_V1: u16 = 0x4859; // "HY"

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
        let checksum = Self::compute_checksum(epoch, &class_str, policy_version, base_offset, &payload);
        Self {
            magic: RECORD_MAGIC_V1,
            epoch,
            class_id: class_str,
            policy_version,
            base_offset,
            payload,
            checksum,
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
        let computed = Self::compute_checksum(
            self.epoch,
            &self.class_id,
            self.policy_version,
            self.base_offset,
            &self.payload,
        );
        computed == self.checksum
    }
}

/// In-memory and persistent partition log manager enforcing INV-1 and INV-4.
#[derive(Clone)]
pub struct PartitionLog {
    inner: Arc<RwLock<PartitionLogInner>>,
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

            let mut cursor = std::io::Cursor::new(buf);
            while let Ok(envelope) = bincode_deserialize_envelope(&mut cursor) {
                if !envelope.verify_checksum() {
                    return Err(LogError::ChecksumMismatch {
                        offset: envelope.base_offset,
                        computed: 0,
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
            }
        }

        let high_watermark = records.len() as u64;

        Ok(Self {
            inner: Arc::new(RwLock::new(PartitionLogInner {
                dir: dir_buf,
                current_epoch: initial_epoch,
                next_offset,
                high_watermark,
                records,
            })),
        })
    }

    /// Appends a payload batch, enforcing INV-1 (monotonic dense offsets) and epoch fencing (§3.4.2).
    pub fn append(
        &self,
        epoch: u64,
        class_id: impl Into<String>,
        policy_version: u64,
        payload: Vec<u8>,
    ) -> Result<u64, LogError> {
        let mut inner = self.inner.write().unwrap();

        // Epoch fencing check
        if epoch < inner.current_epoch {
            return Err(LogError::StaleEpoch {
                current: inner.current_epoch,
                requested: epoch,
            });
        }
        if epoch > inner.current_epoch {
            inner.current_epoch = epoch;
        }

        let offset = inner.next_offset;
        let envelope = RecordEnvelope::new(epoch, class_id, policy_version, offset, payload);

        // Append to file
        let log_path = inner.dir.join("partition.data");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        let serialized = serde_json::to_vec(&envelope).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let len_bytes = (serialized.len() as u32).to_be_bytes();
        file.write_all(&len_bytes)?;
        file.write_all(&serialized)?;
        file.flush()?;

        inner.records.push(envelope);
        inner.next_offset += 1;
        inner.high_watermark += 1;

        Ok(offset)
    }

    /// Fetches records starting at `start_offset`, capped strictly by the High-Water Mark (INV-4).
    pub fn read(&self, start_offset: u64, max_records: usize) -> Vec<RecordEnvelope> {
        let inner = self.inner.read().unwrap();
        if start_offset >= inner.high_watermark {
            return Vec::new();
        }

        let count = std::cmp::min(
            max_records,
            (inner.high_watermark - start_offset) as usize,
        );

        inner.records[start_offset as usize..start_offset as usize + count].to_vec()
    }

    pub fn next_offset(&self) -> u64 {
        self.inner.read().unwrap().next_offset
    }

    pub fn high_watermark(&self) -> u64 {
        self.inner.read().unwrap().high_watermark
    }
}

fn bincode_deserialize_envelope(cursor: &mut std::io::Cursor<Vec<u8>>) -> Result<RecordEnvelope, std::io::Error> {
    let mut len_bytes = [0u8; 4];
    cursor.read_exact(&mut len_bytes)?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    serde_json::from_slice(&buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monotonic_offset_and_crc_verification() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log = PartitionLog::open(temp_dir.path(), 1).unwrap();

        let o0 = log.append(1, "EU-class", 1, b"first payload".to_vec()).unwrap();
        assert_eq!(o0, 0);

        let o1 = log.append(1, "EU-class", 1, b"second payload".to_vec()).unwrap();
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

        let err = log.append(4, "EU-class", 1, b"stale write".to_vec()).unwrap_err();
        assert!(matches!(err, LogError::StaleEpoch { current: 5, requested: 4 }));
    }
}
