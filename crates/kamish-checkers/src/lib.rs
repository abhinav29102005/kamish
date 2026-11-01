//! Invariant Checkers, History Auditors, and Canary Leak Scanner for Kamish (W30).
//!
//! Complies with `project.md` and Phase 4 Master Plan:
//! - FR-10: Canary scanner and invariant checkers.
//! - §3.5.3: Core Invariants Checkers Table (INV-1 through INV-9).
//! - W30: History-based loss/duplicate/order/stale-epoch checkers from independent
//!   producer-ACK and consumer logs; canary scanner over real decoded storage and wire bytes with positive control.

pub mod calibration;
pub use calibration::{
    CalibrationConfig, CalibrationResult, ImpairedTcpProxy, calibrate_transport,
};

use kamish_log::RecordEnvelope;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CheckerViolation {
    #[error(
        "INV-1 Violation: Non-dense offset sequence in stream {topic}:{partition}: expected {expected}, found {found}"
    )]
    OrderingGap {
        topic: String,
        partition: u32,
        expected: u64,
        found: u64,
    },

    #[error(
        "INV-7 Violation: Stale/conflicting epoch write in stream {topic}:{partition}: prior highest epoch was {prior_epoch}, but found stale epoch {stale_epoch} at offset {offset}"
    )]
    EpochConflict {
        topic: String,
        partition: u32,
        offset: u64,
        prior_epoch: u64,
        stale_epoch: u64,
    },

    #[error(
        "INV-8 Canary Leak Violation: Canary token '{canary_token}' detected in unauthorized region '{region}' at {location}"
    )]
    CanaryLeak {
        canary_token: String,
        region: String,
        location: String,
    },

    #[error(
        "INV-2 Data Loss Violation: Acknowledged write at offset {offset} (epoch {epoch}) missing from consumer log for {topic}:{partition}"
    )]
    DataLoss {
        topic: String,
        partition: u32,
        offset: u64,
        epoch: u64,
    },

    #[error(
        "INV-1/INV-6 Duplicate Violation: Duplicate record detected at offset {offset} in stream {topic}:{partition}"
    )]
    DuplicateRecord {
        topic: String,
        partition: u32,
        offset: u64,
    },
}

/// Independent Producer Acknowledgment Log Entry (W30).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerAckRecord {
    pub topic: String,
    pub partition: u32,
    pub producer_id: u64,
    pub sequence: u32,
    pub offset: u64,
    pub epoch: u64,
    pub payload_hash: Vec<u8>,
}

/// Independent Consumer Received Log Entry (W30).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumerRecord {
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub epoch: u64,
    pub payload_hash: Vec<u8>,
}

/// Summary of an invariant verification run across producer and consumer histories.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryReport {
    pub total_acked_records: usize,
    pub total_consumed_records: usize,
    pub verified_streams: usize,
}

/// History-based cross-referencing checker verifying Loss, Duplicates, Ordering, and Stale Epochs (W30).
pub struct HistoryChecker;

impl HistoryChecker {
    /// Cross-checks producer acknowledgment history against consumer logs across all streams and partitions.
    pub fn verify(
        producer_acks: &[ProducerAckRecord],
        consumer_records: &[ConsumerRecord],
    ) -> Result<HistoryReport, CheckerViolation> {
        let mut streams = HashSet::new();

        // 1. Group consumer records by (topic, partition)
        let mut consumed_by_stream: HashMap<(String, u32), Vec<&ConsumerRecord>> = HashMap::new();
        for rec in consumer_records {
            streams.insert((rec.topic.clone(), rec.partition));
            consumed_by_stream
                .entry((rec.topic.clone(), rec.partition))
                .or_default()
                .push(rec);
        }

        // 2. Validate Consumer Stream Invariants: Ordering (INV-1), Duplicates, and Epoch Monotonicity (INV-7)
        for ((topic, partition), records) in &consumed_by_stream {
            let mut highest_epoch = 0u64;
            let mut seen_offsets = HashSet::new();

            for (idx, rec) in records.iter().enumerate() {
                let expected_offset = idx as u64;
                // Check duplicate offsets
                if !seen_offsets.insert(rec.offset) {
                    return Err(CheckerViolation::DuplicateRecord {
                        topic: topic.clone(),
                        partition: *partition,
                        offset: rec.offset,
                    });
                }

                // Check dense monotonically increasing offset sequence
                if rec.offset != expected_offset {
                    return Err(CheckerViolation::OrderingGap {
                        topic: topic.clone(),
                        partition: *partition,
                        expected: expected_offset,
                        found: rec.offset,
                    });
                }

                // Check epoch monotonicity (INV-7)
                if rec.epoch < highest_epoch {
                    return Err(CheckerViolation::EpochConflict {
                        topic: topic.clone(),
                        partition: *partition,
                        offset: rec.offset,
                        prior_epoch: highest_epoch,
                        stale_epoch: rec.epoch,
                    });
                }
                highest_epoch = rec.epoch;
            }
        }

        // 3. Cross-Check Producer ACKs against Consumer Records: Data Loss (INV-2)
        // Every single producer-acknowledged write MUST exist in consumer records with matching offset & epoch.
        for ack in producer_acks {
            streams.insert((ack.topic.clone(), ack.partition));
            let stream_consumed = consumed_by_stream
                .get(&(ack.topic.clone(), ack.partition))
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

            let matched = stream_consumed
                .iter()
                .any(|c| c.offset == ack.offset && c.epoch == ack.epoch);

            if !matched {
                return Err(CheckerViolation::DataLoss {
                    topic: ack.topic.clone(),
                    partition: ack.partition,
                    offset: ack.offset,
                    epoch: ack.epoch,
                });
            }
        }

        Ok(HistoryReport {
            total_acked_records: producer_acks.len(),
            total_consumed_records: consumer_records.len(),
            verified_streams: streams.len(),
        })
    }
}

/// In-memory and Decoded Storage/Wire Canary Scanner with Positive Control (W30, INV-8).
#[derive(Clone)]
pub struct CanaryScanner {
    canary_tokens: HashMap<String, String>, // token string -> authorized legal region
}

impl Default for CanaryScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl CanaryScanner {
    pub fn new() -> Self {
        Self {
            canary_tokens: HashMap::new(),
        }
    }

    /// Registers a sensitive canary token that is ONLY legally authorized in `legal_region`.
    pub fn register_canary(&mut self, token: impl Into<String>, legal_region: impl Into<String>) {
        self.canary_tokens.insert(token.into(), legal_region.into());
    }

    /// Scans arbitrary wire or memory payload bytes against registered canaries.
    pub fn scan_region_payload(
        &self,
        region: &str,
        raw_bytes: &[u8],
    ) -> Result<(), CheckerViolation> {
        self.scan_bytes(region, raw_bytes, "wire buffer / in-memory payload")
    }

    /// Scans raw bytes and reports violation if a forbidden canary token is present.
    pub fn scan_bytes(
        &self,
        region: &str,
        bytes: &[u8],
        location: &str,
    ) -> Result<(), CheckerViolation> {
        for (token, legal_region) in &self.canary_tokens {
            if region != legal_region && bytes.windows(token.len()).any(|w| w == token.as_bytes()) {
                return Err(CheckerViolation::CanaryLeak {
                    canary_token: token.clone(),
                    region: region.to_string(),
                    location: location.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Scans decoded storage records in a physical partition directory (§4.4, W30).
    pub fn scan_partition_dir<P: AsRef<Path>>(
        &self,
        partition_dir: P,
        current_region: &str,
    ) -> Result<usize, CheckerViolation> {
        let dir = partition_dir.as_ref();
        let mut total_scanned = 0;

        // Read partition log directly from disk without locking exclusive partition writer
        let data_file = dir.join("partition.data");
        if data_file.exists()
            && let Ok(bytes) = std::fs::read(&data_file)
        {
            let records = kamish_log::decode_envelopes_from_bytes(&bytes);
            for record in &records {
                total_scanned += 1;
                let loc = format!(
                    "{}/partition.data offset {}",
                    dir.display(),
                    record.base_offset
                );
                self.scan_bytes(current_region, &record.payload, &loc)?;
            }
        }

        // Also scan quarantine file if present
        let quarantine_file = dir.join("quarantine.data");
        if quarantine_file.exists()
            && let Ok(bytes) = std::fs::read(&quarantine_file)
        {
            let loc = format!("{}/quarantine.data", dir.display());
            self.scan_bytes(current_region, &bytes, &loc)?;
        }

        Ok(total_scanned)
    }

    /// Recursively scans an entire broker storage root directory for forbidden canary leakage (W30).
    pub fn scan_broker_storage<P: AsRef<Path>>(
        &self,
        storage_root: P,
        broker_region: &str,
    ) -> Result<usize, CheckerViolation> {
        let root = storage_root.as_ref();
        let mut total_records = 0;

        if !root.exists() {
            return Ok(0);
        }

        for entry in std::fs::read_dir(root).map_err(|_| CheckerViolation::CanaryLeak {
            canary_token: "IO_ERROR".into(),
            region: broker_region.into(),
            location: root.display().to_string(),
        })? {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                total_records += self.scan_partition_dir(&path, broker_region)?;
            }
        }

        Ok(total_records)
    }
}

/// Legacy OrderingChecker for INV-1 backwards-compatibility.
pub struct OrderingChecker;
impl OrderingChecker {
    pub fn verify_partition_records(
        partition: u32,
        records: &[RecordEnvelope],
    ) -> Result<(), CheckerViolation> {
        for (expected, record) in records.iter().enumerate() {
            let expected = expected as u64;
            if record.base_offset != expected {
                return Err(CheckerViolation::OrderingGap {
                    topic: "legacy".into(),
                    partition,
                    expected,
                    found: record.base_offset,
                });
            }
        }
        Ok(())
    }
}

/// Legacy EpochAuditor for INV-7 backwards-compatibility.
pub struct EpochAuditor;
impl EpochAuditor {
    pub fn verify_epoch_monotonicity(
        partition: u32,
        records: &[RecordEnvelope],
    ) -> Result<(), CheckerViolation> {
        let mut highest_epoch = 0;
        for record in records {
            if record.epoch < highest_epoch {
                return Err(CheckerViolation::EpochConflict {
                    topic: "legacy".into(),
                    partition,
                    offset: record.base_offset,
                    prior_epoch: highest_epoch,
                    stale_epoch: record.epoch,
                });
            }
            highest_epoch = record.epoch;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kamish_log::PartitionLog;

    #[test]
    fn test_history_checker_clean_history() {
        let acks = vec![
            ProducerAckRecord {
                topic: "orders".into(),
                partition: 0,
                producer_id: 1,
                sequence: 0,
                offset: 0,
                epoch: 1,
                payload_hash: vec![1, 2, 3],
            },
            ProducerAckRecord {
                topic: "orders".into(),
                partition: 0,
                producer_id: 1,
                sequence: 1,
                offset: 1,
                epoch: 1,
                payload_hash: vec![4, 5, 6],
            },
        ];

        let consumed = vec![
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 0,
                epoch: 1,
                payload_hash: vec![1, 2, 3],
            },
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 1,
                epoch: 1,
                payload_hash: vec![4, 5, 6],
            },
        ];

        let report = HistoryChecker::verify(&acks, &consumed).unwrap();
        assert_eq!(report.total_acked_records, 2);
        assert_eq!(report.total_consumed_records, 2);
    }

    #[test]
    fn test_history_checker_detects_data_loss() {
        let acks = vec![
            ProducerAckRecord {
                topic: "orders".into(),
                partition: 0,
                producer_id: 1,
                sequence: 0,
                offset: 0,
                epoch: 1,
                payload_hash: vec![1],
            },
            ProducerAckRecord {
                topic: "orders".into(),
                partition: 0,
                producer_id: 1,
                sequence: 1,
                offset: 1,
                epoch: 1,
                payload_hash: vec![2],
            },
        ];

        // Consumer only saw record 0, record 1 was lost
        let consumed = vec![ConsumerRecord {
            topic: "orders".into(),
            partition: 0,
            offset: 0,
            epoch: 1,
            payload_hash: vec![1],
        }];

        let err = HistoryChecker::verify(&acks, &consumed).unwrap_err();
        assert!(matches!(err, CheckerViolation::DataLoss { offset: 1, .. }));
    }

    #[test]
    fn test_history_checker_detects_ordering_gap() {
        let consumed = vec![
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 0,
                epoch: 1,
                payload_hash: vec![1],
            },
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 2, // Gap! Missing 1
                epoch: 1,
                payload_hash: vec![2],
            },
        ];

        let err = HistoryChecker::verify(&[], &consumed).unwrap_err();
        assert!(matches!(
            err,
            CheckerViolation::OrderingGap {
                expected: 1,
                found: 2,
                ..
            }
        ));
    }

    #[test]
    fn test_history_checker_detects_duplicate_record() {
        let consumed = vec![
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 0,
                epoch: 1,
                payload_hash: vec![1],
            },
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 0, // Duplicate offset!
                epoch: 1,
                payload_hash: vec![1],
            },
        ];

        let err = HistoryChecker::verify(&[], &consumed).unwrap_err();
        assert!(matches!(
            err,
            CheckerViolation::DuplicateRecord { offset: 0, .. }
        ));
    }

    #[test]
    fn test_history_checker_detects_stale_epoch() {
        let consumed = vec![
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 0,
                epoch: 2,
                payload_hash: vec![1],
            },
            ConsumerRecord {
                topic: "orders".into(),
                partition: 0,
                offset: 1,
                epoch: 1, // Stale! Epoch went 2 -> 1
                payload_hash: vec![2],
            },
        ];

        let err = HistoryChecker::verify(&[], &consumed).unwrap_err();
        assert!(matches!(
            err,
            CheckerViolation::EpochConflict {
                prior_epoch: 2,
                stale_epoch: 1,
                ..
            }
        ));
    }

    #[test]
    fn test_canary_scanner_positive_control() {
        // POSITIVE CONTROL: verify that intentional leak is unequivocally caught on real decoded storage bytes
        let temp_dir = tempfile::tempdir().unwrap();
        let partition_dir = temp_dir.path().join("topic-0");
        std::fs::create_dir_all(&partition_dir).unwrap();

        let mut scanner = CanaryScanner::new();
        scanner.register_canary("CANARY_GDPR_SECRET_987", "EU_WEST");

        // 1. Write an actual partition file with normal data in US_EAST
        let log = PartitionLog::open(&partition_dir, 1).unwrap();
        log.append(1, "general", 1, b"benign public order".to_vec())
            .unwrap();

        // Should pass clean scan in US_EAST
        assert_eq!(
            scanner
                .scan_partition_dir(&partition_dir, "US_EAST")
                .unwrap(),
            1
        );

        // 2. Positive Control: deliberately append record containing EU canary into US_EAST partition
        log.append(
            1,
            "general",
            1,
            b"confidential payload with CANARY_GDPR_SECRET_987 leaked".to_vec(),
        )
        .unwrap();

        // 3. Scan must trigger CanaryLeak violation!
        let err = scanner
            .scan_partition_dir(&partition_dir, "US_EAST")
            .unwrap_err();
        match err {
            CheckerViolation::CanaryLeak {
                canary_token,
                region,
                ..
            } => {
                assert_eq!(canary_token, "CANARY_GDPR_SECRET_987");
                assert_eq!(region, "US_EAST");
            }
            other => panic!("Expected CanaryLeak, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_w31_transport_delay_calibration() {
        let config = CalibrationConfig {
            configured_delay_ms: 25.0, // 25ms delay
            jitter_ms: 2.0,
            loss_rate: 0.0,
            seed: 12345,
            sample_count: 15,
        };

        let result = calibrate_transport(config)
            .await
            .expect("Calibration should succeed");
        assert_eq!(result.sample_count, 15);
        assert_eq!(result.configured_delay_ms, 25.0);

        // Achieved median should be close to 25ms (within 8ms margin of error for local loopback)
        assert!(
            result.p50_median_ms >= 23.0 && result.p50_median_ms <= 35.0,
            "Achieved median was {:.2} ms, expected ~25 ms",
            result.p50_median_ms
        );

        // Spread metrics should be computed
        assert!(result.spread_iqr_ms >= 0.0);
        assert!(result.spread_stddev_ms >= 0.0);
        assert_eq!(result.raw_samples.len(), 15);
    }
}
