//! Regression tests for outbox reservation commit lifecycle, capacity bounds,
//! exact pre-allocation, post-append WAL consistency, and recovery.
//!
//! Verifies:
//! 1. A record whose actual serialized size exceeds the original estimate.
//! 2. Outbox saturation at the reservation/commit boundary.
//! 3. Confirmation that no durable record requiring replication becomes permanently untracked.
//! 4. Correct behavior when capacity reservation fails before WAL append.
//! 5. Reservation accounting after success, failure, and cancellation.
//! 6. Restart recovery of records with outstanding replication obligations.
//! 7. Concurrent producers competing for limited capacity.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::{BrokerServer, ProduceRequest};
use kamish_enforce::Enforcer;
use kamish_log::RecordContext;
use kamish_meta::{KamishPolicyEngine, MetadataStore, ResidencyPolicy};
use kamish_outbox::{BackpressureRule, DestinationOutbox, OutboxError, OutboxRange};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

fn setup_test_broker(
    local_region: &str,
    peers: Vec<String>,
) -> (Arc<BrokerServer>, tempfile::TempDir) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let mut legal = HashSet::new();
    legal.insert(local_region.to_string());
    for peer in &peers {
        legal.insert(peer.clone());
    }

    let mut modes = HashMap::new();
    modes.insert(local_region.to_string(), DestinationMode::Cleartext);
    for peer in &peers {
        modes.insert(peer.clone(), DestinationMode::Cleartext);
    }

    let mut classes = HashMap::new();
    classes.insert(
        "replicated-stream".to_string(),
        ClassRule {
            class_id: "replicated-stream".to_string(),
            description: "Replicable topic".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec![local_region.to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-resilience".to_string(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 5,
        ttl_seconds: 3600,
        issuer_pubkey_hex: pubkey_hex.clone(),
        classes,
        transit_rules: HashMap::new(),
    };

    let signed = SignedPolicy::sign(policy_doc, &signing_key);
    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex);

    let mut cache = PolicyCache::new(trusted);
    cache.update(signed, Utc::now().timestamp()).unwrap();

    let audit_log = AuditLog::new_in_memory();
    let enforcer = Arc::new(Enforcer::new(
        local_region,
        Arc::new(RwLock::new(cache)),
        audit_log.clone(),
    ));
    let meta = Arc::new(MetadataStore::new());
    let temp_dir = tempfile::tempdir().unwrap();

    let mut broker = BrokerServer::new(local_region, enforcer, meta, peers, temp_dir.path());
    let engine = Arc::new(KamishPolicyEngine::new(audit_log));
    broker.set_policy_engine(engine);

    (Arc::new(broker), temp_dir)
}

// ----------------------------------------------------------------------------
// TEST 1, 2 & 3: RECORD SIZE EXCEEDING ESTIMATE & SATURATION AT COMMIT BOUNDARY
// ----------------------------------------------------------------------------

#[test]
fn test_record_size_exceeding_estimate_retains_durable_wal_obligation() {
    let (broker, _td) = setup_test_broker("EU_WEST", vec!["US_EAST".to_string()]);
    let outbox = broker.get_outbox("US_EAST").unwrap();

    // Configure outbox with 120 bytes max capacity
    outbox.set_max_bytes(120);

    // 1. Simulate an initial reservation estimate of 50 bytes
    let reservation = outbox
        .reserve_capacity(50)
        .expect("Initial 50-byte reservation should succeed");
    assert_eq!(outbox.queued_bytes(), 50);

    // 2. Competing reservation claims 50 bytes, leaving only 20 bytes available (120 - 100 = 20)
    let competing_res = outbox
        .reserve_capacity(50)
        .expect("Competing 50-byte reservation should succeed");
    assert_eq!(outbox.queued_bytes(), 100);

    // 3. Durably append record to local WAL at offset 0
    let context = RecordContext {
        producer_id: 100,
        producer_epoch: 1,
        sequence: 0,
        origin_region: "EU_WEST".to_string(),
        request_policy: "{\"residency_rule\":\"GlobalCleartext\"}".to_string(),
        replication_targets: vec!["US_EAST".to_string()],
        source_generation: 1,
    };
    let part_dir = _td.path().join("audit-events-0");
    std::fs::create_dir_all(&part_dir).unwrap();
    let log = kamish_log::PartitionLog::open(&part_dir, 1).unwrap();
    let offset = log
        .append_with_context(1, "replicated-stream", 1, vec![b'A'; 36], Some(context))
        .expect("Direct WAL append should succeed");
    assert_eq!(offset, 0);
    // Drop direct handle so broker can open partition log without conflict (W14)
    drop(log);

    // 4. Actual range size is 100 bytes (payload 36 + 64), which exceeds the 50-byte estimate by 50 bytes.
    // The available capacity is only 20 bytes, so commit growth fails with QueueCapacityExceeded!
    let actual_range = OutboxRange {
        topic: "audit-events".to_string(),
        partition: 0,
        epoch: 1,
        start_offset: 0,
        end_offset: 1,
        class_id: "replicated-stream".to_string(),
        payload: vec![b'A'; 36], // 36 + 64 = 100 bytes
        is_sealed: false,
        source_generation: 1,
        envelope_payload: None,
        ..Default::default()
    };
    assert!(!reservation.is_infallible(&actual_range));

    let commit_err = reservation.commit(actual_range);
    assert!(
        matches!(commit_err, Err(OutboxError::QueueCapacityExceeded { .. })),
        "Commit growth beyond capacity must return QueueCapacityExceeded"
    );

    // Dropping failed reservation releases the 50 bytes; only competing 50 bytes remain
    assert_eq!(outbox.queued_bytes(), 50);

    // INVARIANT AUDIT:
    // 1. Record 0 is durable in the local WAL
    let fetch = broker
        .fetch(kamish_broker::FetchRequest {
            topic: "audit-events".to_string(),
            partition: 0,
            class_id: "replicated-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch.records.len(), 1);
    assert_eq!(fetch.records[0].base_offset, 0);

    // 2. Replication watermark is still 0 (obligation is NOT satisfied)
    assert_eq!(outbox.durable_partition_watermark("audit-events", 0), 0);

    // 3. Competing reservation is released, freeing up outbox capacity
    drop(competing_res);
    assert_eq!(outbox.queued_bytes(), 0);

    // 4. Backlog recovery (repopulate_outbox_from_log) autonomously discovers the unreplicated WAL record
    // and enqueues it without data loss!
    let refilled = broker
        .repopulate_outbox_from_log("audit-events", 0, "US_EAST")
        .unwrap();
    assert_eq!(
        refilled, 1,
        "Backlog recovery must stage the unreplicated WAL record"
    );

    let queued = outbox.next_transfer_batch(10);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].start_offset, 0);
    assert_eq!(queued[0].end_offset, 1);
    assert_eq!(outbox.queued_bytes(), 100);
}

// ----------------------------------------------------------------------------
// TEST 2: EXACT PRE-ALLOCATION GUARANTEES INFALLIBLE COMMIT
// ----------------------------------------------------------------------------

#[test]
fn test_exact_preallocation_guarantees_infallible_commit() {
    let (broker, _td) = setup_test_broker("EU_WEST", vec!["US_EAST".to_string()]);
    let outbox = broker.get_outbox("US_EAST").unwrap();
    outbox.set_max_bytes(500);

    let payload = vec![b'X'; 100];
    let required = OutboxRange::required_capacity(payload.len());
    assert_eq!(required, 164);

    let reservation = outbox.reserve_capacity(required).unwrap();
    assert_eq!(outbox.queued_bytes(), 164);

    let range = OutboxRange {
        topic: "events".to_string(),
        partition: 0,
        epoch: 1,
        start_offset: 0,
        end_offset: 1,
        class_id: "replicated-stream".to_string(),
        payload: payload.clone(),
        is_sealed: false,
        source_generation: 1,
        envelope_payload: None,
        ..Default::default()
    };

    assert!(reservation.is_infallible(&range));
    reservation
        .commit(range)
        .expect("Commit with exact pre-allocation must never fail");
    assert_eq!(outbox.queued_bytes(), 164);
    assert_eq!(outbox.next_transfer_batch(10).len(), 1);
}

// ----------------------------------------------------------------------------
// TEST 4: CAPACITY RESERVATION FAILURE BEFORE WAL APPEND
// ----------------------------------------------------------------------------

#[test]
fn test_capacity_reservation_fails_before_wal_append() {
    let (broker, _td) = setup_test_broker("EU_WEST", vec!["US_EAST".to_string()]);
    let outbox = broker.get_outbox("US_EAST").unwrap();

    // Capacity fits exactly 1 record of 164 bytes
    outbox.set_max_bytes(164);

    // 1. First record fills capacity
    let req1 = ProduceRequest {
        topic: "strict-bound".to_string(),
        partition: 0,
        producer_id: 1,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-stream".to_string(),
        payload: vec![b'A'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res1 = broker.produce(req1).unwrap();
    assert!(res1.sovereignty_allowed);
    assert_eq!(res1.base_offset, 0);
    assert_eq!(outbox.queued_bytes(), 164);

    // 2. Second record rejected BEFORE appending to WAL
    let req2 = ProduceRequest {
        topic: "strict-bound".to_string(),
        partition: 0,
        producer_id: 2,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-stream".to_string(),
        payload: vec![b'B'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res2 = broker.produce(req2).unwrap();
    assert!(!res2.sovereignty_allowed);
    assert!(
        res2.denial_reason
            .contains("Backpressure: destination outbox for 'US_EAST' full")
    );

    // 3. Local log on disk only contains the admitted first record
    let fetch = broker
        .fetch(kamish_broker::FetchRequest {
            topic: "strict-bound".to_string(),
            partition: 0,
            class_id: "replicated-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch.records.len(), 1);
    assert_eq!(fetch.records[0].payload, vec![b'A'; 100]);

    // 4. Zero accounting drift
    assert_eq!(outbox.queued_bytes(), 164);
}

// ----------------------------------------------------------------------------
// TEST 5: RESERVATION ACCOUNTING AFTER SUCCESS, FAILURE, AND CANCELLATION
// ----------------------------------------------------------------------------

#[test]
fn test_reservation_accounting_lifecycle_exhaustive() {
    let outbox = DestinationOutbox::new("US_EAST", 500, BackpressureRule::Reject);
    assert_eq!(outbox.queued_bytes(), 0);

    // Phase 1: Successful commit
    let res1 = outbox.reserve_capacity(164).unwrap();
    assert_eq!(outbox.queued_bytes(), 164);
    let r1 = OutboxRange {
        topic: "t".to_string(),
        partition: 0,
        epoch: 1,
        start_offset: 0,
        end_offset: 1,
        class_id: "c".to_string(),
        payload: vec![0; 100],
        is_sealed: false,
        source_generation: 1,
        envelope_payload: None,
        ..Default::default()
    };
    res1.commit(r1).unwrap();
    assert_eq!(outbox.queued_bytes(), 164);

    // Phase 2: Cancellation (RAII drop uncommitted)
    let res2 = outbox.reserve_capacity(200).unwrap();
    assert_eq!(outbox.queued_bytes(), 364);
    drop(res2);
    assert_eq!(
        outbox.queued_bytes(),
        164,
        "Dropped uncommitted reservation must roll back bytes"
    );

    // Phase 3: Commit growth failure
    let res3 = outbox.reserve_capacity(100).unwrap();
    assert_eq!(outbox.queued_bytes(), 264);
    // Remaining available: 500 - 264 = 236
    // Range size 400 bytes requires additional 300 bytes > 236 available -> Fails!
    let r3_oversized = OutboxRange {
        topic: "t".to_string(),
        partition: 0,
        epoch: 1,
        start_offset: 1,
        end_offset: 2,
        class_id: "c".to_string(),
        payload: vec![0; 336], // 336 + 64 = 400 bytes
        is_sealed: false,
        source_generation: 1,
        envelope_payload: None,
        ..Default::default()
    };
    assert!(res3.commit(r3_oversized).is_err());
    assert_eq!(
        outbox.queued_bytes(),
        164,
        "Failed commit must roll back reserved capacity via drop"
    );

    // Phase 4: Watermark drain
    outbox.advance_partition_watermark("t", 0, 1);
    assert_eq!(
        outbox.queued_bytes(),
        0,
        "Draining acknowledged watermark resets queued bytes to 0"
    );
}

// ----------------------------------------------------------------------------
// TEST 6: RESTART RECOVERY OF UNREPLICATED OBLIGATIONS FROM DURABLE WAL
// ----------------------------------------------------------------------------

#[test]
fn test_restart_recovery_of_unreplicated_obligations() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let mut legal = HashSet::new();
    legal.insert("EU_WEST".to_string());
    legal.insert("US_EAST".to_string());

    let mut modes = HashMap::new();
    modes.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
    modes.insert("US_EAST".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "replicated-stream".to_string(),
        ClassRule {
            class_id: "replicated-stream".to_string(),
            description: "Replicable".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["EU_WEST".to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-restart".to_string(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 5,
        ttl_seconds: 3600,
        issuer_pubkey_hex: pubkey_hex.clone(),
        classes,
        transit_rules: HashMap::new(),
    };
    let signed = SignedPolicy::sign(policy_doc, &signing_key);
    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex);

    let temp_dir = tempfile::tempdir().unwrap();
    let data_path = temp_dir.path().to_path_buf();

    // 1. Initial broker instance: Produce 3 records
    {
        let mut cache = PolicyCache::new(trusted.clone());
        cache
            .update(signed.clone(), Utc::now().timestamp())
            .unwrap();
        let audit_log = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(
            "EU_WEST",
            Arc::new(RwLock::new(cache)),
            audit_log.clone(),
        ));
        let meta = Arc::new(MetadataStore::new());
        let broker = BrokerServer::new(
            "EU_WEST",
            enforcer,
            meta,
            vec!["US_EAST".to_string()],
            &data_path,
        );

        for i in 0..3 {
            let req = ProduceRequest {
                topic: "orders".to_string(),
                partition: 0,
                producer_id: 10,
                producer_epoch: 1,
                sequence_base: i,
                class_id: "replicated-stream".to_string(),
                payload: format!("order-payload-{}", i).into_bytes(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            };
            broker.produce(req).unwrap();
        }

        // Simulate crash: broker dropped while watermark is still 0 (no records replicated yet)
    }

    // 2. Broker restart on same data directory
    {
        let mut cache = PolicyCache::new(trusted);
        cache.update(signed, Utc::now().timestamp()).unwrap();
        let audit_log = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(
            "EU_WEST",
            Arc::new(RwLock::new(cache)),
            audit_log.clone(),
        ));
        let meta = Arc::new(MetadataStore::new());
        let broker = BrokerServer::new(
            "EU_WEST",
            enforcer,
            meta,
            vec!["US_EAST".to_string()],
            &data_path,
        );

        // Verify startup recovery automatically reconstructed all 3 unreplicated records into outbox
        let outbox = broker.get_outbox("US_EAST").unwrap();
        let batch = outbox.next_transfer_batch(10);
        assert_eq!(
            batch.len(),
            3,
            "Restart recovery must automatically stage all 3 unreplicated records"
        );
        assert_eq!(batch[0].start_offset, 0);
        assert_eq!(batch[1].start_offset, 1);
        assert_eq!(batch[2].start_offset, 2);
    }
}

// ----------------------------------------------------------------------------
// TEST 7: CONCURRENT PRODUCERS COMPETING UNDER LIMITED CAPACITY
// ----------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_producers_competing_under_bounded_capacity() {
    let (broker, _td) = setup_test_broker("EU_WEST", vec!["US_EAST".to_string()]);
    // 500 bytes fits exactly 3 records of 164 bytes
    broker.set_outbox_capacity("US_EAST", 500);

    let num_producers = 12;
    let barrier = Arc::new(tokio::sync::Barrier::new(num_producers));
    let mut handles = Vec::new();

    for i in 0..num_producers {
        let b = broker.clone();
        let bar = barrier.clone();
        handles.push(tokio::spawn(async move {
            let req = ProduceRequest {
                topic: "stress".to_string(),
                partition: 0,
                producer_id: 1000 + i as u64,
                producer_epoch: 1,
                sequence_base: 0,
                class_id: "replicated-stream".to_string(),
                payload: vec![b'Z'; 100],
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            };
            bar.wait().await;
            b.produce(req)
        }));
    }

    let mut admitted = 0;
    let mut rejected = 0;

    for h in handles {
        let res = h.await.unwrap().unwrap();
        if res.sovereignty_allowed {
            admitted += 1;
        } else {
            rejected += 1;
            assert!(res.denial_reason.contains("Backpressure"));
        }
    }

    assert_eq!(admitted, 3, "Exactly 3 producers should be admitted");
    assert_eq!(rejected, 9, "Exactly 9 producers should be rejected");

    let outbox = broker.get_outbox("US_EAST").unwrap();
    assert_eq!(outbox.queued_bytes(), 3 * 164);
    assert_eq!(outbox.next_transfer_batch(100).len(), 3);
}

// ----------------------------------------------------------------------------
// TEST 8: MULTI-DESTINATION ATOMIC ADMISSION AND RESERVATION CONSISTENCY
// ----------------------------------------------------------------------------

#[test]
fn test_multi_destination_partial_saturation_atomic_rollback() {
    let (broker, _td) = setup_test_broker(
        "EU_WEST",
        vec!["US_EAST".to_string(), "AP_SOUTH".to_string()],
    );

    // US_EAST has ample space (10,000 bytes)
    broker.set_outbox_capacity("US_EAST", 10_000);
    // AP_SOUTH has space for only 1 record (200 bytes; 1 record = 164 bytes)
    broker.set_outbox_capacity("AP_SOUTH", 200);

    // 1. First record: Fits in both US_EAST and AP_SOUTH
    let req1 = ProduceRequest {
        topic: "multi-dest".to_string(),
        partition: 0,
        producer_id: 2001,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-stream".to_string(),
        payload: vec![b'A'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res1 = broker.produce(req1).unwrap();
    assert!(res1.sovereignty_allowed);
    assert_eq!(res1.base_offset, 0);

    assert_eq!(broker.get_outbox("US_EAST").unwrap().queued_bytes(), 164);
    assert_eq!(broker.get_outbox("AP_SOUTH").unwrap().queued_bytes(), 164);

    // 2. Second record: Fits in US_EAST (164 + 164 = 328 <= 10,000)
    // BUT exceeds AP_SOUTH (164 + 164 = 328 > 200)
    // Atomic admission must reject BEFORE local WAL append and roll back US_EAST reservation!
    let req2 = ProduceRequest {
        topic: "multi-dest".to_string(),
        partition: 0,
        producer_id: 2002,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-stream".to_string(),
        payload: vec![b'B'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res2 = broker.produce(req2).unwrap();
    assert!(!res2.sovereignty_allowed);
    assert!(
        res2.denial_reason
            .contains("Backpressure: destination outbox for 'AP_SOUTH' full")
    );

    // 3. Invariant: US_EAST reservation rolled back; zero accounting drift in either outbox
    assert_eq!(broker.get_outbox("US_EAST").unwrap().queued_bytes(), 164);
    assert_eq!(broker.get_outbox("AP_SOUTH").unwrap().queued_bytes(), 164);

    // 4. Invariant: Local partition log only contains the first record
    let fetch = broker
        .fetch(kamish_broker::FetchRequest {
            topic: "multi-dest".to_string(),
            partition: 0,
            class_id: "replicated-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch.records.len(), 1);
    assert_eq!(fetch.records[0].base_offset, 0);
}
