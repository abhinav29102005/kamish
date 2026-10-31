//! Adversarial Concurrency and Admission Control Regression Test Suite
//!
//! Validates:
//! - Multi-threaded concurrent producer outbox saturation race.
//! - Atomic reservation and rollback across multi-destination replication pipelines.
//! - Prevention of silent data loss (INV-3, INV-5).
//! - Producer retry after backpressure rejection (no duplicates, INV-6).
//! - Broker restart with pending replication obligations repopulated from disk.
//! - Destination drain and recovery after saturation.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::{BrokerServer, ProduceRequest};
use kamish_enforce::Enforcer;
use kamish_meta::{KamishPolicyEngine, MetadataStore, ResidencyPolicy};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

fn setup_broker(local_region: &str, peers: Vec<String>) -> (Arc<BrokerServer>, tempfile::TempDir) {
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
        "replicated-topic".to_string(),
        ClassRule {
            class_id: "replicated-topic".to_string(),
            description: "Replicable across regions".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec![local_region.to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-test".to_string(),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_concurrent_producers_outbox_saturation_zero_silent_drop() {
    let (broker, _td) = setup_broker("EU_WEST", vec!["US_EAST".to_string()]);

    // Outbox range size: 100 bytes payload + 64 bytes metadata = 164 bytes
    // Set US_EAST outbox capacity to 500 bytes -> strictly fits max 3 records (3 * 164 = 492 bytes <= 500)
    broker.set_outbox_capacity("US_EAST", 500);

    let num_producers = 12;
    let mut handles = Vec::new();

    for i in 0..num_producers {
        let b = broker.clone();
        let handle = tokio::spawn(async move {
            let req = ProduceRequest {
                topic: "orders".to_string(),
                partition: 0,
                producer_id: 1000 + i as u64,
                producer_epoch: 1,
                sequence_base: 0,
                class_id: "replicated-topic".to_string(),
                payload: vec![b'X'; 100],
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            };
            b.produce(req)
        });
        handles.push(handle);
    }

    let mut accepted_count = 0;
    let mut rejected_count = 0;

    for h in handles {
        let res = h.await.unwrap().unwrap();
        if res.sovereignty_allowed {
            accepted_count += 1;
            assert_eq!(res.denial_reason, "ok");
        } else {
            rejected_count += 1;
            assert!(
                res.denial_reason
                    .contains("Backpressure: destination outbox for 'US_EAST' full"),
                "Expected backpressure denial reason, got: {}",
                res.denial_reason
            );
        }
    }

    assert_eq!(accepted_count + rejected_count, num_producers);
    // Capacity of 500 bytes can fit exactly 3 records of 164 bytes
    assert_eq!(
        accepted_count, 3,
        "Expected exactly 3 records admitted under bounded capacity"
    );
    assert_eq!(
        rejected_count, 9,
        "Expected 9 records rejected with backpressure"
    );

    // CRITICAL ADVERSARIAL AUDIT CHECK:
    // Verify that every single accepted record is actually in the outbox!
    // No silent data loss!
    let outbox = broker.get_outbox("US_EAST").unwrap();
    let queued_ranges = outbox.next_transfer_batch(100);
    assert_eq!(
        queued_ranges.len(),
        accepted_count,
        "Outbox queued count must strictly match accepted count (Zero silent drops)"
    );

    // Verify local partition log on disk only contains the admitted records
    let fetch_res = broker
        .fetch(kamish_broker::FetchRequest {
            topic: "orders".to_string(),
            partition: 0,
            class_id: "replicated-topic".to_string(),
            start_offset: 0,
            max_records: 100,
        })
        .unwrap();
    assert_eq!(
        fetch_res.records.len(),
        accepted_count,
        "Local partition log must only commit admitted records"
    );
}

#[test]
fn test_multi_destination_partial_saturation_atomic_rollback() {
    let (broker, _td) = setup_broker(
        "EU_WEST",
        vec!["US_EAST".to_string(), "AP_SOUTH".to_string()],
    );

    // US_EAST has ample space (10,000 bytes)
    broker.set_outbox_capacity("US_EAST", 10_000);
    // AP_SOUTH has space for only 1 record (200 bytes; 1 record = 164 bytes)
    broker.set_outbox_capacity("AP_SOUTH", 200);

    // 1. First record: Fits in both US_EAST and AP_SOUTH
    let req1 = ProduceRequest {
        topic: "telemetry".to_string(),
        partition: 0,
        producer_id: 2001,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-topic".to_string(),
        payload: vec![b'A'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res1 = broker.produce(req1).unwrap();
    assert!(res1.sovereignty_allowed);
    assert_eq!(res1.base_offset, 0);

    // 2. Second record: Fits in US_EAST, but exceeds AP_SOUTH (164 + 164 = 328 > 200)
    let req2 = ProduceRequest {
        topic: "telemetry".to_string(),
        partition: 0,
        producer_id: 2002,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-topic".to_string(),
        payload: vec![b'B'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res2 = broker.produce(req2).unwrap();

    // Verify rejection before local commit
    assert!(
        !res2.sovereignty_allowed,
        "Produce request must be rejected when AP_SOUTH is saturated"
    );
    assert!(res2.denial_reason.contains("AP_SOUTH"));

    // Verify atomic rollback on US_EAST: US_EAST reservation was dropped!
    let outbox_us = broker.get_outbox("US_EAST").unwrap();
    let outbox_ap = broker.get_outbox("AP_SOUTH").unwrap();

    assert_eq!(
        outbox_us.queued_bytes(),
        164,
        "US_EAST queued bytes must be exactly 164 (reservation rolled back)"
    );
    assert_eq!(
        outbox_ap.queued_bytes(),
        164,
        "AP_SOUTH queued bytes must be exactly 164"
    );

    assert_eq!(outbox_us.next_transfer_batch(10).len(), 1);
    assert_eq!(outbox_ap.next_transfer_batch(10).len(), 1);

    // Verify local disk log was NOT appended with second record
    let fetch_res = broker
        .fetch(kamish_broker::FetchRequest {
            topic: "telemetry".to_string(),
            partition: 0,
            class_id: "replicated-topic".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(
        fetch_res.records.len(),
        1,
        "Local disk commit must NOT occur when multi-destination replication cannot be satisfied"
    );
}

#[test]
fn test_producer_retry_after_rejection_no_duplicates() {
    let (broker, _td) = setup_broker("EU_WEST", vec!["US_EAST".to_string()]);

    // Outbox capacity set to exactly 1 record (200 bytes)
    broker.set_outbox_capacity("US_EAST", 200);

    // 1. Initial message fills capacity
    let req1 = ProduceRequest {
        topic: "payments".to_string(),
        partition: 0,
        producer_id: 3001,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-topic".to_string(),
        payload: vec![b'1'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res1 = broker.produce(req1).unwrap();
    assert!(res1.sovereignty_allowed);
    assert_eq!(res1.base_offset, 0);

    // 2. Second message from producer 3002 is rejected due to saturation
    let req2 = ProduceRequest {
        topic: "payments".to_string(),
        partition: 0,
        producer_id: 3002,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-topic".to_string(),
        payload: vec![b'2'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res2 = broker.produce(req2.clone()).unwrap();
    assert!(!res2.sovereignty_allowed);

    // 3. Remote broker drains outbox (advances watermark to 1)
    let outbox = broker.get_outbox("US_EAST").unwrap();
    outbox.advance_watermark(1).unwrap();
    assert_eq!(outbox.queued_bytes(), 0);

    // 4. Producer retries the exact same request (same producer_id, epoch, sequence_base)
    let res2_retry = broker.produce(req2.clone()).unwrap();
    assert!(
        res2_retry.sovereignty_allowed,
        "Retry after destination drain must succeed"
    );
    assert_eq!(res2_retry.base_offset, 1);
    assert_eq!(res2_retry.denial_reason, "ok");

    // 5. Producer retries AGAIN (simulating lost ACK): Idempotent deduplication kicks in!
    let res2_dup = broker.produce(req2).unwrap();
    assert!(res2_dup.sovereignty_allowed);
    assert_eq!(res2_dup.base_offset, 1);
    assert_eq!(res2_dup.denial_reason, "idempotent_duplicate");

    // Verify local log contains exactly 2 records, 0 duplicates
    let fetch_res = broker
        .fetch(kamish_broker::FetchRequest {
            topic: "payments".to_string(),
            partition: 0,
            class_id: "replicated-topic".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch_res.records.len(), 2);
    assert_eq!(fetch_res.records[0].base_offset, 0);
    assert_eq!(fetch_res.records[1].base_offset, 1);
}

#[test]
fn test_broker_restart_with_pending_replication_obligations() {
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
        "replicated-topic".to_string(),
        ClassRule {
            class_id: "replicated-topic".to_string(),
            description: "Replicable".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["EU_WEST".to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-test".to_string(),
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

    // PHASE 1: Run broker 1, produce 5 records, advance destination watermark only to 2
    {
        let mut cache = PolicyCache::new(trusted.clone());
        cache
            .update(signed.clone(), Utc::now().timestamp())
            .unwrap();
        let audit = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(
            "EU_WEST",
            Arc::new(RwLock::new(cache)),
            audit.clone(),
        ));
        let meta = Arc::new(MetadataStore::new());
        let mut broker = BrokerServer::new(
            "EU_WEST",
            enforcer,
            meta,
            vec!["US_EAST".to_string()],
            temp_dir.path(),
        );
        broker.set_policy_engine(Arc::new(KamishPolicyEngine::new(audit)));

        for i in 0..5 {
            let req = ProduceRequest {
                topic: "audit-stream".to_string(),
                partition: 0,
                producer_id: 4000,
                producer_epoch: 1,
                sequence_base: i,
                class_id: "replicated-topic".to_string(),
                payload: format!("record-{}", i).into_bytes(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            };
            let res = broker.produce(req).unwrap();
            assert_eq!(res.base_offset, i as u64);
        }

        // Simulate US_EAST only having caught up to offset 2
        let outbox = broker.get_outbox("US_EAST").unwrap();
        outbox.advance_partition_watermark("audit-stream", 0, 2);
        assert_eq!(outbox.durable_watermark(), 2);
    }
    // Broker 1 is dropped (simulating dirty kill / process restart)

    // PHASE 2: Start Broker 2 from the exact same disk directory
    {
        let mut cache = PolicyCache::new(trusted);
        cache.update(signed, Utc::now().timestamp()).unwrap();
        let audit = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(
            "EU_WEST",
            Arc::new(RwLock::new(cache)),
            audit.clone(),
        ));
        let meta = Arc::new(MetadataStore::new());
        let mut broker_restarted = BrokerServer::new(
            "EU_WEST",
            enforcer,
            meta,
            vec!["US_EAST".to_string()],
            temp_dir.path(),
        );
        broker_restarted.set_policy_engine(Arc::new(KamishPolicyEngine::new(audit)));

        // Advance restarted outbox watermark to 2 (learned via remote handshake)
        let outbox = broker_restarted.get_outbox("US_EAST").unwrap();
        assert_eq!(
            outbox.next_transfer_batch(10).len(),
            5,
            "Startup must reconstruct pending work without an explicit recovery call"
        );
        outbox.advance_partition_watermark("audit-stream", 0, 2);

        // Repopulate pending replication obligations from disk commit log
        let staged = broker_restarted
            .repopulate_outbox_from_log("audit-stream", 0, "US_EAST")
            .expect("Repopulate should succeed");

        assert_eq!(
            staged, 0,
            "Startup already reconstructed work; a second recovery must be idempotent"
        );

        // Verify the staged ranges match offsets 2, 3, 4
        let batches = outbox.next_transfer_batch(10);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].start_offset, 2);
        assert_eq!(batches[1].start_offset, 3);
        assert_eq!(batches[2].start_offset, 4);

        // Verify idempotency of repopulate
        let staged_again = broker_restarted
            .repopulate_outbox_from_log("audit-stream", 0, "US_EAST")
            .unwrap();
        assert_eq!(
            staged_again, 0,
            "Duplicate repopulate must stage 0 additional records"
        );
    }
}

#[test]
fn test_destination_recovery_after_saturation() {
    let (broker, _td) = setup_broker("EU_WEST", vec!["US_EAST".to_string()]);

    // Set capacity to fit exactly 2 records (350 bytes; 2 * 164 = 328 <= 350)
    broker.set_outbox_capacity("US_EAST", 350);

    // 1. Fill outbox to capacity with 2 records
    for i in 0..2 {
        let req = ProduceRequest {
            topic: "events".to_string(),
            partition: 0,
            producer_id: 5000 + i,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "replicated-topic".to_string(),
            payload: vec![b'E'; 100],
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        };
        let res = broker.produce(req).unwrap();
        assert!(res.sovereignty_allowed);
    }

    // 2. 3rd record is rejected because outbox is saturated
    let req3 = ProduceRequest {
        topic: "events".to_string(),
        partition: 0,
        producer_id: 5002,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "replicated-topic".to_string(),
        payload: vec![b'E'; 100],
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
    };
    let res3 = broker.produce(req3.clone()).unwrap();
    assert!(!res3.sovereignty_allowed);
    assert!(res3.denial_reason.contains("Backpressure"));

    // 3. Destination recovers: Replication worker transfers 2 records and advances watermark to 2
    let outbox = broker.get_outbox("US_EAST").unwrap();
    let batch = outbox.next_transfer_batch(10);
    assert_eq!(batch.len(), 2);
    outbox.advance_watermark(2).unwrap();
    assert_eq!(outbox.queued_bytes(), 0);

    // 4. Ingress immediately recovers and accepts new produce requests
    let res3_recovered = broker.produce(req3).unwrap();
    assert!(res3_recovered.sovereignty_allowed);
    assert_eq!(res3_recovered.base_offset, 2);
    assert_eq!(outbox.next_transfer_batch(10).len(), 1);
}
