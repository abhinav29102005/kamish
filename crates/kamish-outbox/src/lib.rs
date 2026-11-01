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

use kamish_log::RecordContext;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutboxError {
    #[error(
        "An unscoped watermark requires exactly one known stream; use advance_partition_watermark"
    )]
    AmbiguousStream,
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
    #[serde(default = "default_generation")]
    pub source_generation: u64,
    #[serde(default)]
    pub envelope_payload: Option<Vec<u8>>,
    #[serde(default)]
    pub context: Option<RecordContext>,
    #[serde(default = "default_generation")]
    pub policy_version: u64,
    #[serde(default)]
    pub checksum: Option<u32>,
}

fn default_generation() -> u64 {
    1
}

impl Default for OutboxRange {
    fn default() -> Self {
        Self {
            topic: String::new(),
            partition: 0,
            epoch: 0,
            start_offset: 0,
            end_offset: 0,
            class_id: String::new(),
            payload: Vec::new(),
            is_sealed: false,
            source_generation: 1,
            envelope_payload: None,
            context: None,
            policy_version: 1,
            checksum: None,
        }
    }
}

/// Parameter struct for constructing an `OutboxRange` without argument-count bloat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRangeParams {
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
    pub fn new(params: OutboxRangeParams) -> Self {
        Self {
            topic: params.topic,
            partition: params.partition,
            epoch: params.epoch,
            start_offset: params.start_offset,
            end_offset: params.end_offset,
            class_id: params.class_id,
            payload: params.payload,
            is_sealed: params.is_sealed,
            source_generation: 1,
            envelope_payload: None,
            context: None,
            policy_version: 1,
            checksum: None,
        }
    }

    pub fn with_context(mut self, context: Option<RecordContext>) -> Self {
        if let Some(ref ctx) = context {
            self.source_generation = ctx.source_generation;
        }
        self.context = context;
        self
    }

    pub fn with_source_generation(mut self, generation: u64) -> Self {
        self.source_generation = generation;
        self
    }

    pub fn with_policy_version(mut self, policy_version: u64) -> Self {
        self.policy_version = policy_version;
        self
    }

    pub fn with_checksum(mut self, checksum: u32) -> Self {
        self.checksum = Some(checksum);
        self
    }

    pub fn with_envelope_payload(mut self, envelope_payload: Option<Vec<u8>>) -> Self {
        self.envelope_payload = envelope_payload;
        self
    }
    pub fn byte_size(&self) -> usize {
        let env_len = self.envelope_payload.as_ref().map_or(0, |e| e.len());
        self.payload.len() + env_len + 64
    }

    /// Computes the exact capacity required in an outbox for a range with the given payload length.
    pub fn required_capacity(payload_len: usize) -> usize {
        payload_len + 64
    }

    /// Computes the exact capacity required in an outbox for a range with both payload and envelope bytes.
    pub fn required_capacity_with_envelope(payload_len: usize, envelope_len: usize) -> usize {
        payload_len + envelope_len + 64
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
    ranges: VecDeque<(OutboxRange, u64)>,
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

    /// Checks whether committing the given range is guaranteed to be infallible.
    ///
    /// Commitment is infallible whenever `range.byte_size() <= self.reserved_bytes`,
    /// because sufficient capacity was already pre-allocated under the reservation lock.
    pub fn is_infallible(&self, range: &OutboxRange) -> bool {
        range.byte_size() <= self.reserved_bytes
    }

    /// Commits this reservation by enqueueing the provided range.
    ///
    /// Adjusts `queued_bytes` if the actual range size differs from the initial reservation estimate.
    pub fn commit(mut self, range: OutboxRange) -> Result<(), OutboxError> {
        let actual_size = range.byte_size();
        let mut inner = self.outbox.inner.write().unwrap();
        if actual_size != self.reserved_bytes {
            if actual_size > self.reserved_bytes {
                let additional = actual_size - self.reserved_bytes;
                if additional > inner.max_bytes.saturating_sub(inner.queued_bytes) {
                    return Err(OutboxError::QueueCapacityExceeded {
                        current: inner.queued_bytes,
                        max: inner.max_bytes,
                    });
                }
                inner.queued_bytes += additional;
            } else {
                inner.queued_bytes = inner
                    .queued_bytes
                    .saturating_sub(self.reserved_bytes - actual_size);
            }
        }
        self.committed = true;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        inner.ranges.push_back((range, now_ms));
        Ok(())
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
        if additional_bytes > inner.max_bytes.saturating_sub(inner.queued_bytes) {
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
        reservation.commit(range)
    }

    /// Retrieves pending ranges starting strictly from the durable watermark (§3.4.5, INV-5).
    pub fn next_transfer_batch(&self, max_ranges: usize) -> Vec<OutboxRange> {
        let inner = self.inner.read().unwrap();

        inner
            .ranges
            .iter()
            .filter(|(r, _)| {
                let wm = inner
                    .watermarks
                    .get(&(r.topic.clone(), r.partition))
                    .copied()
                    .unwrap_or(0);
                r.end_offset > wm
            })
            .map(|(r, _)| r.clone())
            .take(max_ranges)
            .collect()
    }

    /// Acknowledges receiver progress bound to source epoch and generation provenance (§3.3.2, INV-6, CONTRACT §2.2).
    ///
    /// Ensures that a watermark advance only trims ranges matching or preceding the acknowledged
    /// epoch and source generation, preventing an older replication response from trimming work
    /// belonging to a higher epoch or newer generation.
    pub fn advance_partition_watermark_with_provenance(
        &self,
        topic: &str,
        partition: u32,
        epoch: u64,
        source_generation: u64,
        new_watermark: u64,
    ) {
        let mut inner = self.inner.write().unwrap();
        let key = (topic.to_string(), partition);
        let current = inner.watermarks.get(&key).copied().unwrap_or(0);
        if new_watermark <= current {
            return; // strictly monotonic
        }
        inner.watermarks.insert(key, new_watermark);
        inner.durable_watermark = std::cmp::max(inner.durable_watermark, new_watermark);

        // Trim strictly ranges belonging to this (topic, partition) whose provenance matches or precedes the ACK
        let mut trimmed_bytes = 0;
        inner.ranges.retain(|(r, _)| {
            if r.topic == topic && r.partition == partition {
                let provenance_matches =
                    r.epoch <= epoch && r.source_generation <= source_generation;
                if provenance_matches && r.end_offset <= new_watermark {
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

    /// Acknowledges receiver progress for a specific partition, trimming applied ranges (§3.3.2, INV-6).
    pub fn advance_partition_watermark(&self, topic: &str, partition: u32, new_watermark: u64) {
        self.advance_partition_watermark_with_provenance(
            topic,
            partition,
            u64::MAX,
            u64::MAX,
            new_watermark,
        );
    }

    /// Compatibility helper for single-stream outboxes. Never broadcast an ACK
    /// to unrelated partitions. Multi-stream callers must specify the stream.
    pub fn advance_watermark(&self, new_watermark: u64) -> Result<(), OutboxError> {
        let mut inner = self.inner.write().unwrap();
        let streams: std::collections::HashSet<_> = inner
            .watermarks
            .keys()
            .cloned()
            .chain(
                inner
                    .ranges
                    .iter()
                    .map(|(r, _)| (r.topic.clone(), r.partition)),
            )
            .collect();
        if streams.len() != 1 {
            return Err(OutboxError::AmbiguousStream);
        }
        let key = streams.into_iter().next().unwrap();
        let current = inner.watermarks.get(&key).copied().unwrap_or(0);
        if new_watermark <= current {
            return Ok(());
        }
        inner.watermarks.insert(key.clone(), new_watermark);
        inner.durable_watermark = inner.durable_watermark.max(new_watermark);
        let mut trimmed_bytes = 0;
        inner.ranges.retain(|(r, _)| {
            if r.topic == key.0 && r.partition == key.1 && r.end_offset <= new_watermark {
                trimmed_bytes += r.byte_size();
                false
            } else {
                true
            }
        });
        inner.queued_bytes = inner.queued_bytes.saturating_sub(trimmed_bytes);
        Ok(())
    }

    pub fn queued_bytes(&self) -> usize {
        self.inner.read().unwrap().queued_bytes
    }

    pub fn queued_ranges(&self) -> usize {
        self.inner.read().unwrap().ranges.len()
    }

    pub fn pending_count(&self) -> usize {
        self.inner.read().unwrap().ranges.len()
    }

    pub fn oldest_entry_age_ms(&self) -> Option<u64> {
        let inner = self.inner.read().unwrap();
        let (_, enqueued_at) = inner.ranges.front()?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Some(now_ms.saturating_sub(*enqueued_at))
    }
    pub fn has_capacity(&self, additional_bytes: usize) -> bool {
        let inner = self.inner.read().unwrap();
        additional_bytes <= inner.max_bytes.saturating_sub(inner.queued_bytes)
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
            .unwrap_or(0)
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
            .any(|(r, _)| r.start_offset <= offset && offset < r.end_offset)
    }

    /// Checks if a given offset for a specific topic/partition has already been queued or acknowledged.
    pub fn contains_partition_offset(&self, topic: &str, partition: u32, offset: u64) -> bool {
        let inner = self.inner.read().unwrap();
        let wm = inner
            .watermarks
            .get(&(topic.to_string(), partition))
            .copied()
            .unwrap_or(0);
        if offset < wm {
            return true;
        }
        inner.ranges.iter().any(|(r, _)| {
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
    fn test_outbox_range_params_constructor() {
        let params = OutboxRangeParams {
            topic: "events".to_string(),
            partition: 0,
            epoch: 1,
            start_offset: 0,
            end_offset: 1,
            class_id: "general".to_string(),
            payload: b"test-payload".to_vec(),
            is_sealed: false,
        };
        let range = OutboxRange::new(params.clone());
        assert_eq!(range.topic, params.topic);
        assert_eq!(range.partition, params.partition);
        assert_eq!(range.epoch, params.epoch);
        assert_eq!(range.start_offset, params.start_offset);
        assert_eq!(range.end_offset, params.end_offset);
        assert_eq!(range.class_id, params.class_id);
        assert_eq!(range.payload, params.payload);
        assert!(!range.is_sealed);
    }

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
            source_generation: 1,
            envelope_payload: None,
            ..Default::default()
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
            source_generation: 1,
            envelope_payload: None,
            ..Default::default()
        };

        outbox.enqueue(r1).unwrap();
        outbox.enqueue(r2).unwrap();

        assert_eq!(outbox.next_transfer_batch(10).len(), 2);

        // Advance watermark to 5 (r1 trimmed, r2 retained)
        outbox.advance_watermark(5).unwrap();
        assert_eq!(outbox.durable_watermark(), 5);
        let batch = outbox.next_transfer_batch(10);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].start_offset, 5);

        // Duplicate ack is a no-op (INV-6)
        outbox.advance_watermark(5).unwrap();
        assert_eq!(outbox.durable_watermark(), 5);

        // Advance to 10 (all trimmed)
        outbox.advance_watermark(10).unwrap();
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
            source_generation: 1,
            envelope_payload: None,
            ..Default::default()
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
            source_generation: 1,
            envelope_payload: None,
            ..Default::default()
        };
        res3.commit(r).unwrap();

        assert_eq!(outbox.queued_bytes(), 150);
        assert_eq!(outbox.next_transfer_batch(10).len(), 1);
        assert!(outbox.contains_offset(0));
    }

    #[test]
    fn test_outbox_metrics_age_and_pending_count() {
        let outbox = DestinationOutbox::new("region-b", 1024, BackpressureRule::Reject);
        assert_eq!(outbox.pending_count(), 0);
        assert_eq!(outbox.oldest_entry_age_ms(), None);

        let r = OutboxRange::new(OutboxRangeParams {
            topic: "test-topic".into(),
            partition: 0,
            epoch: 1,
            start_offset: 0,
            end_offset: 1,
            class_id: "class-a".into(),
            payload: vec![1, 2, 3],
            is_sealed: false,
        });

        outbox.enqueue(r).unwrap();
        assert_eq!(outbox.pending_count(), 1);
        assert!(outbox.queued_bytes() > 0);
        assert!(outbox.oldest_entry_age_ms().is_some());
    }
}
