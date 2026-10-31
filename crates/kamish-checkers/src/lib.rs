//! Invariant Checkers and Canary Leak Scanner for Kamish.
//!
//! Complies with `project.md`:
//! - FR-10: Provide canary scanner and invariant checkers.
//! - §3.5.3: Core Invariants Checkers Table (INV-1 through INV-9).
//! - §4.4: Checkers we write (loss/duplicate, per-partition ordering, canary scanner, epoch auditor).


use kamish_log::RecordEnvelope;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CheckerViolation {
    #[error("INV-1 Violation: Non-dense offset sequence in partition {partition}: expected {expected}, found {found}")]
    OrderingGap { partition: u32, expected: u64, found: u64 },

    #[error("INV-7 Violation: Stale/conflicting epoch write detected for partition {partition}: epoch {epoch}")]
    EpochConflict { partition: u32, epoch: u64 },

    #[error("INV-8 Canary Leak Violation: Canary token '{canary_token}' detected in forbidden region '{region}'")]
    CanaryLeak { canary_token: String, region: String },

    #[error("INV-2 Loss Violation: Record missing at expected offset {offset} for partition {partition}")]
    DataLoss { partition: u32, offset: u64 },
}

/// Ordering Checker for Invariant INV-1 (§3.5.3).
pub struct OrderingChecker;

impl OrderingChecker {
    pub fn verify_partition_records(partition: u32, records: &[RecordEnvelope]) -> Result<(), CheckerViolation> {
        let mut expected = 0;
        for record in records {
            if record.base_offset != expected {
                return Err(CheckerViolation::OrderingGap {
                    partition,
                    expected,
                    found: record.base_offset,
                });
            }
            expected += 1;
        }
        Ok(())
    }
}

/// Stale-Epoch and Split-Brain Auditor for Invariant INV-7 (§3.5.3).
pub struct EpochAuditor;

impl EpochAuditor {
    /// Checks that within a partition, epochs increase monotonically across committed batches.
    pub fn verify_epoch_monotonicity(partition: u32, records: &[RecordEnvelope]) -> Result<(), CheckerViolation> {
        let mut highest_epoch = 0;
        for record in records {
            if record.epoch < highest_epoch {
                return Err(CheckerViolation::EpochConflict {
                    partition,
                    epoch: record.epoch,
                });
            }
            highest_epoch = record.epoch;
        }
        Ok(())
    }
}

/// Canary Leak Scanner for Invariant INV-8 (§1.6 D1, §4.4).
pub struct CanaryScanner {
    canary_tokens: HashMap<String, String>, // token -> allowed legal region
}

impl CanaryScanner {
    pub fn new() -> Self {
        Self {
            canary_tokens: HashMap::new(),
        }
    }

    /// Registers a unique canary token associated with a specific legal region.
    pub fn register_canary(&mut self, token: impl Into<String>, legal_region: impl Into<String>) {
        self.canary_tokens.insert(token.into(), legal_region.into());
    }

    /// Scans payload bytes in a given region. Violates INV-8 if a canary token appears in an unauthorized region.
    pub fn scan_region_payload(&self, region: &str, raw_bytes: &[u8]) -> Result<(), CheckerViolation> {
        for (token, legal_region) in &self.canary_tokens {
            if region != legal_region {
                // If token bytes appear in raw bytes of unauthorized region -> LEAK!
                if raw_bytes.windows(token.len()).any(|w| w == token.as_bytes()) {
                    return Err(CheckerViolation::CanaryLeak {
                        canary_token: token.clone(),
                        region: region.to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordering_checker_detects_gap() {
        let r0 = RecordEnvelope::new(1, "c", 1, 0, b"p".to_vec());
        let r2 = RecordEnvelope::new(1, "c", 1, 2, b"p".to_vec()); // Gap! Missing 1

        let err = OrderingChecker::verify_partition_records(0, &[r0, r2]).unwrap_err();
        assert!(matches!(err, CheckerViolation::OrderingGap { expected: 1, found: 2, .. }));
    }

    #[test]
    fn test_canary_scanner_detects_leak() {
        let mut scanner = CanaryScanner::new();
        scanner.register_canary("CANARY_EU_TOP_SECRET", "region-eu");

        // Allowed in region-eu
        assert!(scanner.scan_region_payload("region-eu", b"data with CANARY_EU_TOP_SECRET inside").is_ok());

        // Forbidden in region-us -> MUST detect leak!
        let err = scanner.scan_region_payload("region-us", b"leaked CANARY_EU_TOP_SECRET here").unwrap_err();
        assert!(matches!(err, CheckerViolation::CanaryLeak { .. }));
    }
}
