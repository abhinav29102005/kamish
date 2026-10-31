use kamish_outbox::{BackpressureRule, DestinationOutbox, OutboxError, OutboxRange};

fn range(partition: u32) -> OutboxRange {
    OutboxRange {
        topic: "events".into(),
        partition,
        epoch: 1,
        start_offset: 0,
        end_offset: 1,
        class_id: "class".into(),
        payload: vec![0; 36],
        is_sealed: false,
    }
}

#[test]
fn ack_does_not_hide_a_new_partition_or_topic() {
    let outbox = DestinationOutbox::new("b", 1000, BackpressureRule::Reject);
    outbox.enqueue(range(0)).unwrap();
    outbox.enqueue(range(1)).unwrap();
    outbox.advance_partition_watermark("events", 0, 1);
    assert_eq!(outbox.next_transfer_batch(10), vec![range(1)]);
    assert_eq!(outbox.durable_partition_watermark("events", 1), 0);
    assert!(!outbox.contains_partition_offset("events", 2, 0));
    assert_eq!(outbox.durable_partition_watermark("other-topic", 0), 0);
    assert!(matches!(
        outbox.advance_watermark(2),
        Err(OutboxError::AmbiguousStream)
    ));
    assert_eq!(outbox.next_transfer_batch(10), vec![range(1)]);
}

#[test]
fn unscoped_ack_cannot_initialize_an_unknown_stream() {
    let outbox = DestinationOutbox::new("b", 1000, BackpressureRule::Reject);
    assert!(matches!(
        outbox.advance_watermark(100),
        Err(OutboxError::AmbiguousStream)
    ));
    outbox.enqueue(range(0)).unwrap();
    assert_eq!(outbox.next_transfer_batch(1).len(), 1);
}

#[test]
fn commit_growth_must_fit_capacity_and_failed_commit_releases_reservation() {
    let outbox = DestinationOutbox::new("b", 90, BackpressureRule::Reject);
    let reservation = outbox.reserve_capacity(50).unwrap();
    assert!(matches!(
        reservation.commit(range(0)),
        Err(OutboxError::QueueCapacityExceeded { .. })
    ));
    assert_eq!(outbox.queued_bytes(), 0);
    assert!(outbox.next_transfer_batch(10).is_empty());
}

#[test]
fn arithmetic_overflow_cannot_bypass_capacity() {
    let outbox = DestinationOutbox::new("b", 1000, BackpressureRule::Reject);
    let held = outbox.reserve_capacity(1).unwrap();
    assert!(!outbox.has_capacity(usize::MAX));
    assert!(outbox.reserve_capacity(usize::MAX).is_err());
    drop(held);
    assert_eq!(outbox.queued_bytes(), 0);
}
