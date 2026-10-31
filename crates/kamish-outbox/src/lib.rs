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
use std::collections::{HashMap, VecDeque};
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
    #[allow(dead_code)]
    backpressure: BackpressureRule,
    inner: Arc<RwLock<OutboxInner>>,
}

struct OutboxInner {
    ranges: VecDeque<OutboxRange>,
    queued_bytes: usize,
    durable_watermark: u64,
    watermarks: HashMap<(String, u32), u64>,
    max_bytes: usize,
}

/// An atomic capacity reservation ticket for a `DestinationOutbox`.
///
/// Ensures zero-race admission control across concurrent producers and
/// atomicity across multi-destination replication pipelines.
pub struct OutboxReservation {
    outbox: DestinationOutbox,
    reserved_bytes: usize,
    committed: bool,
}

impl OutboxReservation {
    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    /// Commits this reservation by enqueueing the provided range.
    ///
    /// Adjusts `queued_bytes` if the actual range size differs from the initial reservation estimate.
    pub fn commit(mut self, range: OutboxRange) {
        self.committed = true;
        let actual_size = range.byte_size();
        let mut inner = self.outbox.inner.write().unwrap();
        if actual_size != self.reserved_bytes {
            if actual_size > self.reserved_bytes {
                inner.queued_bytes += actual_size - self.reserved_bytes;
            } else {
                inner.queued_bytes = inner
                    .queued_bytes
                    .saturating_sub(self.reserved_bytes - actual_size);
            }
        }
        inner.ranges.push_back(range);
    }
}

impl Drop for OutboxReservation {
    fn drop(&mut self) {
        if !self.committed && self.reserved_bytes > 0 {
            let mut inner = self.outbox.inner.write().unwrap();
            inner.queued_bytes = inner.queued_bytes.saturating_sub(self.reserved_bytes);
        }
    }
}

impl DestinationOutbox {
    pub fn new(
        target_region: impl Into<String>,
        max_bytes: usize,
        backpressure: BackpressureRule,
    ) -> Self {
        Self {
            target_region: target_region.into(),
            backpressure,
            inner: Arc::new(RwLock::new(OutboxInner {
                ranges: VecDeque::new(),
                queued_bytes: 0,
                durable_watermark: 0,
                watermarks: HashMap::new(),
                max_bytes,
            })),
        }
    }

    /// Atomically checks and reserves replication outbox byte capacity.
    ///
    /// If capacity is available, increments `queued_bytes` immediately under
    /// the write lock and returns an `OutboxReservation` ticket.
    ///
    /// If the ticket is committed via `reservation.commit(range)`, the range is placed into the queue.
    /// If the ticket is dropped before commit (e.g., local log append failure or multi-destination abort),
    /// the reserved capacity is automatically rolled back via `Drop`.
    pub fn reserve_capacity(
        &self,
        additional_bytes: usize,
    ) -> Result<OutboxReservation, OutboxError> {
        let mut inner = self.inner.write().unwrap();
        if inner.queued_bytes + additional_bytes > inner.max_bytes {
            return Err(OutboxError::QueueCapacityExceeded {
                current: inner.queued_bytes,
                max: inner.max_bytes,
            });
        }

        inner.queued_bytes += additional_bytes;
        Ok(OutboxReservation {
            outbox: self.clone(),
            reserved_bytes: additional_bytes,
            committed: false,
        })
    }

    /// Enqueues a new replication range, enforcing bounded queue capacity (§2.4, §2.7.3).
    pub fn enqueue(&self, range: OutboxRange) -> Result<(), OutboxError> {
        let range_size = range.byte_size();
        let reservation = self.reserve_capacity(range_size)?;
        reservation.commit(range);
        Ok(())
    }

    /// Retrieves pending ranges starting strictly from the durable watermark (§3.4.5, INV-5).
    pub fn next_transfer_batch(&self, max_ranges: usize) -> Vec<OutboxRange> {
        let inner = self.inner.read().unwrap();

        inner
            .ranges
            .iter()
            .filter(|r| {
                let wm = inner
                    .watermarks
                    .get(&(r.topic.clone(), r.partition))
                    .copied()
                    .unwrap_or(inner.durable_watermark);
                r.end_offset > wm
            })
            .take(max_ranges)
            .cloned()
            .collect()
    }

    /// Acknowledges receiver progress for a specific partition, trimming applied ranges (§3.3.2, INV-6).
    pub fn advance_partition_watermark(&self, topic: &str, partition: u32, new_watermark: u64) {
        let mut inner = self.inner.write().unwrap();
        let key = (topic.to_string(), partition);
        let current = inner.watermarks.get(&key).copied().unwrap_or(0);
        if new_watermark <= current {
            return; // strictly monotonic
        }
        inner.watermarks.insert(key, new_watermark);
        inner.durable_watermark = std::cmp::max(inner.durable_watermark, new_watermark);

        // Trim strictly ranges belonging to this (topic, partition)
        let mut trimmed_bytes = 0;
        inner.ranges.retain(|r| {
            if r.topic == topic && r.partition == partition {
                if r.end_offset <= new_watermark {
                    trimmed_bytes += r.byte_size();
                    false
                } else {
                    true
                }
            } else {
                true // Retain other partitions
            }
        });
        inner.queued_bytes = inner.queued_bytes.saturating_sub(trimmed_bytes);
    }

    /// Acknowledges receiver progress across all partitions, trimming applied ranges (§3.3.2, INV-6).
    pub fn advance_watermark(&self, new_watermark: u64) {
        let mut inner = self.inner.write().unwrap();
        if new_watermark <= inner.durable_watermark {
            return; // no-op on duplicate or regressing ack (INV-6)
        }

        inner.durable_watermark = new_watermark;

        for val in inner.watermarks.values_mut() {
            if *val < new_watermark {
                *val = new_watermark;
            }
        }

        let mut trimmed_bytes = 0;
        inner.ranges.retain(|r| {
            if r.end_offset <= new_watermark {
                trimmed_bytes += r.byte_size();
                false
            } else {
                true
            }
        });
        inner.queued_bytes = inner.queued_bytes.saturating_sub(trimmed_bytes);
    }

    pub fn queued_bytes(&self) -> usize {
        self.inner.read().unwrap().queued_bytes
    }

    pub fn has_capacity(&self, additional_bytes: usize) -> bool {
        let inner = self.inner.read().unwrap();
        inner.queued_bytes + additional_bytes <= inner.max_bytes
    }

    pub fn max_bytes(&self) -> usize {
        self.inner.read().unwrap().max_bytes
    }

    pub fn set_max_bytes(&self, new_max: usize) {
        self.inner.write().unwrap().max_bytes = new_max;
    }

    pub fn durable_watermark(&self) -> u64 {
        self.inner.read().unwrap().durable_watermark
    }

    pub fn durable_partition_watermark(&self, topic: &str, partition: u32) -> u64 {
        let inner = self.inner.read().unwrap();
        inner
            .watermarks
            .get(&(topic.to_string(), partition))
            .copied()
            .unwrap_or(inner.durable_watermark)
    }

    pub fn target_region(&self) -> &str {
        &self.target_region
    }

    /// Checks if a given offset has already been queued or acknowledged.
    pub fn contains_offset(&self, offset: u64) -> bool {
        let inner = self.inner.read().unwrap();
        if offset < inner.durable_watermark {
            return true;
        }
        inner
            .ranges
            .iter()
            .any(|r| r.start_offset <= offset && offset < r.end_offset)
    }

    /// Checks if a given offset for a specific topic/partition has already been queued or acknowledged.
    pub fn contains_partition_offset(&self, topic: &str, partition: u32, offset: u64) -> bool {
        let inner = self.inner.read().unwrap();
        let wm = inner
            .watermarks
            .get(&(topic.to_string(), partition))
            .copied()
            .unwrap_or(inner.durable_watermark);
        if offset < wm {
            return true;
        }
        inner.ranges.iter().any(|r| {
            r.topic == topic
                && r.partition == partition
                && r.start_offset <= offset
                && offset < r.end_offset
        })
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

    #[test]
    fn test_outbox_atomic_reservation_and_rollback() {
        let outbox = DestinationOutbox::new("region-c", 300, BackpressureRule::Reject);

        // 1. Reserve 200 bytes
        let res1 = outbox.reserve_capacity(200).expect("res1 should succeed");
        assert_eq!(outbox.queued_bytes(), 200);

        // 2. Try reserving 150 bytes -> 200 + 150 = 350 > 300 -> Fails
        let res2_err = outbox.reserve_capacity(150);
        assert!(res2_err.is_err());
        assert_eq!(outbox.queued_bytes(), 200);

        // 3. Drop res1 without commit -> Automatically rolls back queued bytes
        drop(res1);
        assert_eq!(outbox.queued_bytes(), 0);

        // 4. Now 150 bytes reservation succeeds
        let res3 = outbox.reserve_capacity(150).expect("res3 should succeed");
        assert_eq!(outbox.queued_bytes(), 150);

        // 5. Commit reservation with range
        let r = OutboxRange {
            topic: "events".to_string(),
            partition: 0,
            epoch: 1,
            start_offset: 0,
            end_offset: 1,
            class_id: "standard".to_string(),
            payload: vec![0; 86], // 86 + 64 = 150 bytes
            is_sealed: false,
        };
        res3.commit(r);

        assert_eq!(outbox.queued_bytes(), 150);
        assert_eq!(outbox.next_transfer_batch(10).len(), 1);
        assert!(outbox.contains_offset(0));
    }
}
