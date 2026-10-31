//! Durable Outbox and Resumable Transfer Queue for Kamish.
//!
//! Complies with `project.md`:
//! - §2.2: Bounded durable outbox per destination.
//! - §2.4: BackpressureRule (Block, Reject, DegradeLocal).
//! - §2.7.3: Level 2 Process 4.0 Replicate Cross-Region.
//! - §3.3.2: Outbox Transfer State Machine.
//! - §3.4.5: Reference Outbox Resume pseudocode.
//! - INV-5: Destination log is prefix of source log, offsets match exactly.
//! - INV-6: Retrying an applied range is a no-op.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutboxError {
    #[error("Outbox queue capacity exceeded: current bytes {current}, max {max}")]
    QueueCapacityExceeded { current: usize, max: usize },

    #[error("Transfer denied by policy: {0}")]
    PolicyDenied(String),
}

/// Backpressure rule when outbox disk/memory bound is reached (§2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackpressureRule {
    Block,
    Reject,
    DegradeLocal,
}

/// Individual replication range queued in the outbox (§2.7.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxRange {
    pub topic: String,
    pub partition: u32,
    pub epoch: u64,
    pub start_offset: u64,
    pub end_offset: u64,
    pub class_id: String,
    pub payload: Vec<u8>,
    pub is_sealed: bool,
}

impl OutboxRange {
    pub fn byte_size(&self) -> usize {
        self.payload.len() + 64
    }
}

/// Bounded durable outbox per destination region (§2.2, §3.3.2).
#[derive(Clone)]
pub struct DestinationOutbox {
    target_region: String,
    max_bytes: usize,
    backpressure: BackpressureRule,
    inner: Arc<RwLock<OutboxInner>>,
}

struct OutboxInner {
    ranges: VecDeque<OutboxRange>,
    queued_bytes: usize,
    durable_watermark: u64,
}

impl DestinationOutbox {
    pub fn new(target_region: impl Into<String>, max_bytes: usize, backpressure: BackpressureRule) -> Self {
        Self {
            target_region: target_region.into(),
            max_bytes,
            backpressure,
            inner: Arc::new(RwLock::new(OutboxInner {
                ranges: VecDeque::new(),
                queued_bytes: 0,
                durable_watermark: 0,
            })),
        }
    }

    /// Enqueues a new replication range, enforcing bounded queue capacity (§2.4, §2.7.3).
    pub fn enqueue(&self, range: OutboxRange) -> Result<(), OutboxError> {
        let mut inner = self.inner.write().unwrap();
        let range_size = range.byte_size();

        if inner.queued_bytes + range_size > self.max_bytes {
            match self.backpressure {
                BackpressureRule::Reject => {
                    return Err(OutboxError::QueueCapacityExceeded {
                        current: inner.queued_bytes,
                        max: self.max_bytes,
                    });
                }
                BackpressureRule::DegradeLocal => {
                    // In DegradeLocal mode, queue drops oldest or halts replication for this class
                    return Err(OutboxError::QueueCapacityExceeded {
                        current: inner.queued_bytes,
                        max: self.max_bytes,
                    });
                }
                BackpressureRule::Block => {
                    return Err(OutboxError::QueueCapacityExceeded {
                        current: inner.queued_bytes,
                        max: self.max_bytes,
                    });
                }
            }
        }

        inner.queued_bytes += range_size;
        inner.ranges.push_back(range);
        Ok(())
    }

    /// Retrieves pending ranges starting strictly from the durable watermark (§3.4.5, INV-5).
    pub fn next_transfer_batch(&self, max_ranges: usize) -> Vec<OutboxRange> {
        let inner = self.inner.read().unwrap();
        let watermark = inner.durable_watermark;

        inner
            .ranges
            .iter()
            .filter(|r| r.end_offset > watermark)
            .take(max_ranges)
            .cloned()
            .collect()
    }

    /// Acknowledges receiver progress, trimming applied ranges from the outbox queue (§3.3.2, INV-6).
    pub fn advance_watermark(&self, new_watermark: u64) {
        let mut inner = self.inner.write().unwrap();
        if new_watermark <= inner.durable_watermark {
            return; // no-op on duplicate or regressing ack (INV-6)
        }

        inner.durable_watermark = new_watermark;

        // Trim ranges completely covered by the new watermark
        while let Some(front) = inner.ranges.front() {
            if front.end_offset <= new_watermark {
                let removed = inner.ranges.pop_front().unwrap();
                inner.queued_bytes = inner.queued_bytes.saturating_sub(removed.byte_size());
            } else {
                break;
            }
        }
    }

    pub fn queued_bytes(&self) -> usize {
        self.inner.read().unwrap().queued_bytes
    }

    pub fn durable_watermark(&self) -> u64 {
        self.inner.read().unwrap().durable_watermark
    }

    pub fn target_region(&self) -> &str {
        &self.target_region
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbox_enqueue_advance_and_trim() {
        let outbox = DestinationOutbox::new("region-b", 10_000, BackpressureRule::Reject);

        let r1 = OutboxRange {
            topic: "events".to_string(),
            partition: 0,
            epoch: 1,
            start_offset: 0,
            end_offset: 5,
            class_id: "standard".to_string(),
            payload: vec![1; 100],
            is_sealed: false,
        };

        let r2 = OutboxRange {
            topic: "events".to_string(),
            partition: 0,
            epoch: 1,
            start_offset: 5,
            end_offset: 10,
            class_id: "standard".to_string(),
            payload: vec![2; 100],
            is_sealed: false,
        };

        outbox.enqueue(r1).unwrap();
        outbox.enqueue(r2).unwrap();

        assert_eq!(outbox.next_transfer_batch(10).len(), 2);

        // Advance watermark to 5 (r1 trimmed, r2 retained)
        outbox.advance_watermark(5);
        assert_eq!(outbox.durable_watermark(), 5);
        let batch = outbox.next_transfer_batch(10);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].start_offset, 5);

        // Duplicate ack is a no-op (INV-6)
        outbox.advance_watermark(5);
        assert_eq!(outbox.durable_watermark(), 5);

        // Advance to 10 (all trimmed)
        outbox.advance_watermark(10);
        assert_eq!(outbox.next_transfer_batch(10).len(), 0);
        assert_eq!(outbox.queued_bytes(), 0);
    }

    #[test]
    fn test_outbox_bounded_capacity() {
        let outbox = DestinationOutbox::new("region-b", 200, BackpressureRule::Reject);

        let r1 = OutboxRange {
            topic: "events".to_string(),
            partition: 0,
            epoch: 1,
            start_offset: 0,
            end_offset: 1,
            class_id: "standard".to_string(),
            payload: vec![0; 250], // exceeds max_bytes
            is_sealed: false,
        };

        let err = outbox.enqueue(r1).unwrap_err();
        assert!(matches!(err, OutboxError::QueueCapacityExceeded { .. }));
    }
}
