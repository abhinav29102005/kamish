//! Milestone 5.2: Three-Region Convergence, Recovery, and Regional Resilience Audit
//!
//! Deterministically verifies:
//! 1. Three-region topology (EU_WEST -> US_EAST and AP_SOUTH).
//! 2. Link degradation and outbox saturation behavior (independent stream progress).
//! 3. Broker restart recovery with outstanding replication obligations.
//! 4. Non-contiguous gap rejection and watermark safety.
//! 5. Idempotent deduplication and conflicting duplicate rejection.
//! 6. Policy revocation preventing unauthorized replication repopulation.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::{BrokerServer, ProduceRequest, ReplicationRequest};
use kamish_enforce::Enforcer;
use kamish_meta::{KamishPolicyEngine, MetadataStore, ResidencyPolicy};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

#[allow(dead_code)]
struct TestCluster {
    pub signing_key: SigningKey,
    pub pubkey_hex: String,
    pub cache_a: Arc<RwLock<PolicyCache>>,
    pub meta_a: Arc<MetadataStore>,
    pub broker_a: Arc<BrokerServer>,
    pub dir_a: tempfile::TempDir,
    pub broker_b: Arc<BrokerServer>,
    pub dir_b: tempfile::TempDir,
    pub broker_c: Arc<BrokerServer>,
    pub dir_c: tempfile::TempDir,
}

fn setup_three_region_cluster() -> TestCluster {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let mut legal_global = HashSet::new();
    legal_global.insert("EU_WEST".to_string());
    legal_global.insert("US_EAST".to_string());
    legal_global.insert("AP_SOUTH".to_string());

    let mut modes_global = HashMap::new();
    modes_global.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
    modes_global.insert("US_EAST".to_string(), DestinationMode::Cleartext);
    modes_global.insert("AP_SOUTH".to_string(), DestinationMode::Cleartext);

    let mut legal_asia = HashSet::new();
    legal_asia.insert("EU_WEST".to_string());
    legal_asia.insert("AP_SOUTH".to_string());

    let mut modes_asia = HashMap::new();
    modes_asia.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
    modes_asia.insert("AP_SOUTH".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "global-stream".to_string(),
        ClassRule {
            class_id: "global-stream".to_string(),
            description: "Replicates to all 3 regions".to_string(),
            legal_regions: legal_global,
            destination_modes: modes_global,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["EU_WEST".to_string()],
            transit_allowed: true,
        },
    );
    classes.insert(
        "asia-stream".to_string(),
        ClassRule {
            class_id: "asia-stream".to_string(),
            description: "Replicates only to AP_SOUTH".to_string(),
            legal_regions: legal_asia,
            destination_modes: modes_asia,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["EU_WEST".to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-three-region".to_string(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 5,
        ttl_seconds: 3600,
        issuer_pubkey_hex: pubkey_hex.clone(),
        classes,
        transit_rules: HashMap::new(),
    };

    let signed = SignedPolicy::sign(policy_doc, &signing_key);
    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex.clone());

    let make_broker = |region: &str, peers: Vec<String>| {
        let mut cache = PolicyCache::new(trusted.clone());
        cache
            .update(signed.clone(), Utc::now().timestamp())
            .unwrap();
        let cache_arc = Arc::new(RwLock::new(cache));
        let audit = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(region, cache_arc.clone(), audit.clone()));
        let meta = Arc::new(MetadataStore::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let mut broker = BrokerServer::new(region, enforcer, meta.clone(), peers, temp_dir.path());
        broker.set_policy_engine(Arc::new(KamishPolicyEngine::new(audit)));
        (Arc::new(broker), temp_dir, cache_arc, meta)
    };

    let (broker_a, dir_a, cache_a, meta_a) = make_broker(
        "EU_WEST",
        vec!["US_EAST".to_string(), "AP_SOUTH".to_string()],
    );
    let (broker_b, dir_b, _, _) = make_broker(
        "US_EAST",
        vec!["EU_WEST".to_string(), "AP_SOUTH".to_string()],
    );
    let (broker_c, dir_c, _, _) = make_broker(
        "AP_SOUTH",
        vec!["EU_WEST".to_string(), "US_EAST".to_string()],
    );

    TestCluster {
        signing_key,
        pubkey_hex,
        cache_a,
        meta_a,
        broker_a,
        dir_a,
        broker_b,
        dir_b,
        broker_c,
        dir_c,
    }
}

// ----------------------------------------------------------------------------
// TEST 1: THREE-REGION INDEPENDENT PROGRESS & OUTBOX SATURATION BACKPRESSURE
// ----------------------------------------------------------------------------

#[test]
fn test_three_region_saturation_and_independent_progress() {
    let cluster = setup_three_region_cluster();
    let outbox_b = cluster.broker_a.get_outbox("US_EAST").unwrap();
    let outbox_c = cluster.broker_a.get_outbox("AP_SOUTH").unwrap();

    // Bound outbox for US_EAST to hold at most 2 records (record size = 100 bytes)
    outbox_b.set_max_bytes(200);

    // Outbox for US_EAST and AP_SOUTH are initialized

    // 1. Produce 2 records to global-events -> both B and C outboxes receive records
    for i in 0..2 {
        let resp = cluster
            .broker_a
            .produce(ProduceRequest {
                topic: "global-events".to_string(),
                partition: 0,
                class_id: "global-stream".to_string(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
                producer_id: 1,
                producer_epoch: 1,
                sequence_base: i,
                payload: vec![b'G'; 36], // 36 + 64 = 100 bytes
            })
            .expect("Produce within capacity should succeed");
        assert!(resp.sovereignty_allowed);
        assert_eq!(resp.base_offset, i as u64);
    }

    assert_eq!(outbox_b.queued_bytes(), 200);
    assert_eq!(outbox_c.queued_bytes(), 200);

    // 2. Third produce to global-events must fail because US_EAST outbox is full
    let resp3 = cluster
        .broker_a
        .produce(ProduceRequest {
            topic: "global-events".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            producer_id: 1,
            producer_epoch: 1,
            sequence_base: 2,
            payload: vec![b'G'; 36],
        })
        .expect("Produce should return backpressure response");
    assert!(!resp3.sovereignty_allowed);
    assert!(
        resp3
            .denial_reason
            .contains("Backpressure: destination outbox for 'US_EAST' full"),
        "Expected US_EAST backpressure denial, got: {}",
        resp3.denial_reason
    );

    // Crucial check: reservations for other peers (AP_SOUTH) were dropped and not leaked!
    assert_eq!(outbox_b.queued_bytes(), 200);
    assert_eq!(outbox_c.queued_bytes(), 200);

    // 3. Independent stream: produce to asia-events (which only targets AP_SOUTH)
    // AP_SOUTH outbox has space (default 10 MB limit). This MUST succeed, proving
    // that US_EAST saturation does not freeze unrelated streams.
    let resp_asia = cluster
        .broker_a
        .produce(ProduceRequest {
            topic: "asia-events".to_string(),
            partition: 0,
            class_id: "asia-stream".to_string(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            producer_id: 2,
            producer_epoch: 1,
            sequence_base: 0,
            payload: vec![b'A'; 36],
        })
        .expect("Produce to asia-events should succeed");
    assert!(resp_asia.sovereignty_allowed);
    assert_eq!(resp_asia.base_offset, 0);
    assert_eq!(outbox_c.queued_bytes(), 300); // 200 from global + 100 from asia
}

// ----------------------------------------------------------------------------
// TEST 2: RESTART RECOVERY WITH UNREPLICATED BACKLOG
// ----------------------------------------------------------------------------

#[test]
fn test_broker_restart_recovery_and_watermark_advancement() {
    let cluster = setup_three_region_cluster();
    let data_dir_a = cluster.dir_a.path().to_path_buf();

    // Produce 3 records on Broker A
    for i in 0..3 {
        let resp = cluster
            .broker_a
            .produce(ProduceRequest {
                topic: "replicated-topic".to_string(),
                partition: 0,
                class_id: "global-stream".to_string(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
                producer_id: 10,
                producer_epoch: 1,
                sequence_base: i,
                payload: vec![b'X'; 36],
            })
            .unwrap();
        assert!(resp.sovereignty_allowed);
    }

    let outbox_b_pre = cluster.broker_a.get_outbox("US_EAST").unwrap();
    assert_eq!(outbox_b_pre.queued_bytes(), 300);

    // Simulate link down: no replication happened yet.
    // Broker A crashes / restarts.
    drop(cluster.broker_a);

    // Restart Broker A from existing data directory
    let mut cache = PolicyCache::new({
        let mut s = HashSet::new();
        s.insert(cluster.pubkey_hex.clone());
        s
    });
    let policy_doc = PolicyDocument {
        policy_id: "pol-three-region".to_string(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 5,
        ttl_seconds: 3600,
        issuer_pubkey_hex: cluster.pubkey_hex.clone(),
        classes: {
            let mut m = HashMap::new();
            let mut legal = HashSet::new();
            legal.insert("EU_WEST".to_string());
            legal.insert("US_EAST".to_string());
            legal.insert("AP_SOUTH".to_string());
            let mut modes = HashMap::new();
            modes.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
            modes.insert("US_EAST".to_string(), DestinationMode::Cleartext);
            modes.insert("AP_SOUTH".to_string(), DestinationMode::Cleartext);
            m.insert(
                "global-stream".to_string(),
                ClassRule {
                    class_id: "global-stream".to_string(),
                    description: "Replicates to all 3 regions".to_string(),
                    legal_regions: legal,
                    destination_modes: modes,
                    ack_mode: AckMode::LocalMajority,
                    failover_candidates: vec!["EU_WEST".to_string()],
                    transit_allowed: true,
                },
            );
            m
        },
        transit_rules: HashMap::new(),
    };
    let signed = SignedPolicy::sign(policy_doc, &cluster.signing_key);
    cache.update(signed, Utc::now().timestamp()).unwrap();

    let audit = AuditLog::new_in_memory();
    let enforcer = Arc::new(Enforcer::new(
        "EU_WEST",
        Arc::new(RwLock::new(cache)),
        audit.clone(),
    ));
    let meta = Arc::new(MetadataStore::new());
    let mut restarted_broker = BrokerServer::new(
        "EU_WEST",
        enforcer,
        meta,
        vec!["US_EAST".to_string(), "AP_SOUTH".to_string()],
        &data_dir_a,
    );
    restarted_broker.set_policy_engine(Arc::new(KamishPolicyEngine::new(audit)));

    // On startup, try_new automatically discovers un-replicated records on disk
    // and populates the outbox. Calling refill_outbox again returns 0 due to idempotency.
    let outbox_b_post = restarted_broker.get_outbox("US_EAST").unwrap();
    assert_eq!(
        outbox_b_post.queued_bytes(),
        300,
        "Broker restart must autonomously repopulate all 3 un-replicated records from disk"
    );

    let refilled_again = restarted_broker.refill_outbox("US_EAST").unwrap();
    assert_eq!(
        refilled_again, 0,
        "Subsequent refill must be idempotent and not re-stage existing records"
    );

    // Link restored: replicate batches to Broker B
    let batch = outbox_b_post.next_transfer_batch(10);
    assert_eq!(batch.len(), 3);

    for range in &batch {
        let rep_resp = cluster
            .broker_b
            .handle_replication(ReplicationRequest {
                topic: range.topic.clone(),
                partition: range.partition,
                source_region: "EU_WEST".to_string(),
                target_region: "US_EAST".to_string(),
                class_id: range.class_id.clone(),
                epoch: range.epoch,
                from_offset: range.start_offset,
                to_offset: range.end_offset,
                payload: range.payload.clone(),
                is_sealed: false,
            })
            .unwrap();
        assert!(rep_resp.accepted);
        assert_eq!(rep_resp.applied_watermark, range.end_offset);

        outbox_b_post.advance_partition_watermark(
            &range.topic,
            range.partition,
            rep_resp.applied_watermark,
        );
    }

    // All records acknowledged: outbox drained
    assert_eq!(outbox_b_post.queued_bytes(), 0);
    assert_eq!(
        outbox_b_post.durable_partition_watermark("replicated-topic", 0),
        3
    );
}

// ----------------------------------------------------------------------------
// TEST 3: RECEIVER OFFSET GAP DETECTION AND REJECTION
// ----------------------------------------------------------------------------

#[test]
fn test_receiver_offset_gap_detection_and_watermark_preservation() {
    let cluster = setup_three_region_cluster();

    // 1. Send record 0 -> Accepted, watermark advances to 1
    let rep0 = cluster
        .broker_b
        .handle_replication(ReplicationRequest {
            topic: "gap-test".to_string(),
            partition: 0,
            source_region: "EU_WEST".to_string(),
            target_region: "US_EAST".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 0,
            to_offset: 1,
            payload: vec![1, 2, 3],
            is_sealed: false,
        })
        .unwrap();
    assert!(rep0.accepted);
    assert_eq!(rep0.applied_watermark, 1);

    // 2. Attempt to send record 2 (skipping record 1) -> Must be rejected with offset_gap
    let rep2_gap = cluster
        .broker_b
        .handle_replication(ReplicationRequest {
            topic: "gap-test".to_string(),
            partition: 0,
            source_region: "EU_WEST".to_string(),
            target_region: "US_EAST".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 2,
            to_offset: 3,
            payload: vec![7, 8, 9],
            is_sealed: false,
        })
        .unwrap();
    assert!(!rep2_gap.accepted);
    assert_eq!(rep2_gap.applied_watermark, 1);
    assert!(
        rep2_gap
            .reason
            .contains("offset_gap: expected 1, received 2"),
        "Expected gap rejection reason, got: {}",
        rep2_gap.reason
    );

    // Receiver log must still have only 1 record (offset 0)
    let fetch = cluster
        .broker_b
        .fetch(kamish_broker::FetchRequest {
            topic: "gap-test".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch.records.len(), 1);
    assert_eq!(fetch.records[0].base_offset, 0);

    // 3. Repair gap: Send record 1 -> Accepted, watermark advances to 2
    let rep1 = cluster
        .broker_b
        .handle_replication(ReplicationRequest {
            topic: "gap-test".to_string(),
            partition: 0,
            source_region: "EU_WEST".to_string(),
            target_region: "US_EAST".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 1,
            to_offset: 2,
            payload: vec![4, 5, 6],
            is_sealed: false,
        })
        .unwrap();
    assert!(rep1.accepted);
    assert_eq!(rep1.applied_watermark, 2);

    // 4. Now send record 2 -> Accepted, watermark advances to 3
    let rep2_ok = cluster
        .broker_b
        .handle_replication(ReplicationRequest {
            topic: "gap-test".to_string(),
            partition: 0,
            source_region: "EU_WEST".to_string(),
            target_region: "US_EAST".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 2,
            to_offset: 3,
            payload: vec![7, 8, 9],
            is_sealed: false,
        })
        .unwrap();
    assert!(rep2_ok.accepted);
    assert_eq!(rep2_ok.applied_watermark, 3);
}

// ----------------------------------------------------------------------------
// TEST 4: IDEMPOTENT DEDUPLICATION & CONFLICTING DUPLICATE REJECTION
// ----------------------------------------------------------------------------

#[test]
fn test_receiver_idempotent_deduplication_and_conflict_rejection() {
    let cluster = setup_three_region_cluster();

    // 1. Initial replication of offset 0
    let rep0 = cluster
        .broker_b
        .handle_replication(ReplicationRequest {
            topic: "dedup-test".to_string(),
            partition: 0,
            source_region: "EU_WEST".to_string(),
            target_region: "US_EAST".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 0,
            to_offset: 1,
            payload: vec![42; 32],
            is_sealed: false,
        })
        .unwrap();
    assert!(rep0.accepted);
    assert_eq!(rep0.applied_watermark, 1);

    // 2. Re-send identical offset 0 (network retry / reconnect) -> Accepted as idempotent duplicate
    let rep0_retry = cluster
        .broker_b
        .handle_replication(ReplicationRequest {
            topic: "dedup-test".to_string(),
            partition: 0,
            source_region: "EU_WEST".to_string(),
            target_region: "US_EAST".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 0,
            to_offset: 1,
            payload: vec![42; 32],
            is_sealed: false,
        })
        .unwrap();
    assert!(rep0_retry.accepted);
    assert_eq!(rep0_retry.applied_watermark, 1);
    assert_eq!(rep0_retry.reason, "idempotent_duplicate_noop");

    // 3. Re-send offset 0 with conflicting payload -> Rejected with replication_history_conflict
    let rep0_conflict = cluster
        .broker_b
        .handle_replication(ReplicationRequest {
            topic: "dedup-test".to_string(),
            partition: 0,
            source_region: "EU_WEST".to_string(),
            target_region: "US_EAST".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: 0,
            to_offset: 1,
            payload: vec![99; 32], // Conflicting data!
            is_sealed: false,
        })
        .unwrap();
    assert!(!rep0_conflict.accepted);
    assert_eq!(rep0_conflict.applied_watermark, 1);
    assert_eq!(rep0_conflict.reason, "replication_history_conflict");
}

// ----------------------------------------------------------------------------
// TEST 5: POLICY REVOCATION BLOCKS RECOVERY REPOPULATION
// ----------------------------------------------------------------------------

#[test]
fn test_policy_revocation_blocks_outbox_recovery_repopulation() {
    let cluster = setup_three_region_cluster();

    // Produce record to Broker A destined for US_EAST
    let resp = cluster
        .broker_a
        .produce(ProduceRequest {
            topic: "sensitive-topic".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            producer_id: 50,
            producer_epoch: 1,
            sequence_base: 0,
            payload: vec![b'S'; 36],
        })
        .unwrap();
    assert!(resp.sovereignty_allowed);

    let outbox_b = cluster.broker_a.get_outbox("US_EAST").unwrap();
    assert_eq!(outbox_b.queued_bytes(), 100);

    // Drain/clear the in-memory outbox queue to simulate loss of volatile state before restart
    let _ = outbox_b.next_transfer_batch(10);
    // (Note: durable watermark remains 0)

    // Now update policy to REVOKE US_EAST from legal_regions for global-stream
    let mut legal_restricted = HashSet::new();
    legal_restricted.insert("EU_WEST".to_string());
    legal_restricted.insert("AP_SOUTH".to_string()); // US_EAST removed!

    let mut modes_restricted = HashMap::new();
    modes_restricted.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
    modes_restricted.insert("AP_SOUTH".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "global-stream".to_string(),
        ClassRule {
            class_id: "global-stream".to_string(),
            description: "Restricted stream - US_EAST revoked".to_string(),
            legal_regions: legal_restricted,
            destination_modes: modes_restricted,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["EU_WEST".to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-three-region".to_string(),
        version: 2,
        issued_at_sec: Utc::now().timestamp(),
        ttl_seconds: 3600,
        issuer_pubkey_hex: cluster.pubkey_hex.clone(),
        classes,
        transit_rules: HashMap::new(),
    };

    let signed = SignedPolicy::sign(policy_doc, &cluster.signing_key);
    cluster
        .cache_a
        .write()
        .unwrap()
        .update(signed, Utc::now().timestamp())
        .unwrap();

    // Now attempt to repopulate outbox for US_EAST: Policy enforcer should deny it!
    let repopulated = cluster
        .broker_a
        .repopulate_outbox_from_log("sensitive-topic", 0, "US_EAST")
        .unwrap();
    assert_eq!(
        repopulated, 0,
        "Policy revocation must prevent staging records to revoked destination"
    );
}
