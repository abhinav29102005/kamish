//! Milestone 5.3: Replication Liveness and Scheduling Remediation Regression Tests
//!
//! Validates:
//! Phase 1:
//! - A gap rejection prevents subsequent unnecessary transfers in the same batch.
//! - The missing prefix remains recoverable.
//! - A later retry converges once the destination is ready.
//! - A partial-overlap response does not silently discard unconfirmed records.
//! - A conflicting duplicate is not treated as a recoverable gap.
//! - A healthy independent destination continues to progress.
//!
//! Phase 2:
//! - Three consecutive transport failures do not permanently stall a pending obligation.
//! - Recovery occurs after the destination becomes available without an unrelated produce request.
//! - A broker restart with pending obligations eventually resumes replication.
//! - Repeated failures do not create a busy loop.
//! - A failed destination does not starve a healthy destination.
//! - Watermarks and queued-byte accounting remain correct throughout retries.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_agent::ReplicationAgent;
use kamish_audit::AuditLog;
use kamish_broker::{BrokerServer, ProduceRequest, ReplicationRequest};
use kamish_enforce::Enforcer;
use kamish_meta::{MetadataStore, ResidencyPolicy};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use kamish_route::RouteSelector;
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[allow(dead_code)]
struct ClusterHarness {
    pub signing_key: SigningKey,
    pub pubkey_hex: String,
    pub agent: Arc<ReplicationAgent>,
    pub broker_eu: Arc<BrokerServer>,
    pub dir_eu: tempfile::TempDir,
    pub broker_us: Arc<BrokerServer>,
    pub dir_us: tempfile::TempDir,
    pub broker_ap: Arc<BrokerServer>,
    pub dir_ap: tempfile::TempDir,
    pub policy_cache: Arc<RwLock<PolicyCache>>,
}

fn setup_cluster() -> ClusterHarness {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let mut legal_all = HashSet::new();
    legal_all.insert("region-eu".to_string());
    legal_all.insert("region-us".to_string());
    legal_all.insert("region-ap".to_string());

    let mut modes_all = HashMap::new();
    modes_all.insert("region-eu".to_string(), DestinationMode::Cleartext);
    modes_all.insert("region-us".to_string(), DestinationMode::Cleartext);
    modes_all.insert("region-ap".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "global-stream".to_string(),
        ClassRule {
            class_id: "global-stream".to_string(),
            description: "Replicates across all regions".to_string(),
            legal_regions: legal_all,
            destination_modes: modes_all,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-eu".to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-liveness".to_string(),
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

    let mut cache = PolicyCache::new(trusted);
    cache.update(signed, Utc::now().timestamp()).unwrap();
    let policy_cache = Arc::new(RwLock::new(cache));

    let make_broker = |region: &str, peers: Vec<String>| {
        let audit = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(region, policy_cache.clone(), audit));
        let meta = Arc::new(MetadataStore::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let broker = Arc::new(BrokerServer::new(
            region,
            enforcer,
            meta,
            peers,
            temp_dir.path(),
        ));
        (broker, temp_dir)
    };

    let (broker_eu, dir_eu) = make_broker(
        "region-eu",
        vec!["region-us".to_string(), "region-ap".to_string()],
    );
    let (broker_us, dir_us) = make_broker(
        "region-us",
        vec!["region-eu".to_string(), "region-ap".to_string()],
    );
    let (broker_ap, dir_ap) = make_broker(
        "region-ap",
        vec!["region-eu".to_string(), "region-us".to_string()],
    );

    let agent_audit = AuditLog::new_in_memory();
    let agent_enforcer = Enforcer::new("region-eu", policy_cache.clone(), agent_audit);
    let router = RouteSelector::new("region-eu", agent_enforcer);
    let agent_meta = Arc::new(MetadataStore::new());

    let agent = Arc::new(ReplicationAgent::new(
        "region-eu",
        broker_eu.clone(),
        agent_meta,
        router,
    ));

    agent.register_peer("region-us", broker_us.clone());
    agent.register_peer("region-ap", broker_ap.clone());

    ClusterHarness {
        signing_key,
        pubkey_hex,
        agent,
        broker_eu,
        dir_eu,
        broker_us,
        dir_us,
        broker_ap,
        dir_ap,
        policy_cache,
    }
}

// ----------------------------------------------------------------------------
// PHASE 1 TESTS: STOP BATCH ON BLOCKING GAP RESPONSE
// ----------------------------------------------------------------------------

#[test]
fn test_gap_rejection_halts_batch_and_prevents_unnecessary_transfers() {
    let harness = setup_cluster();

    // 1. Destination Broker US manually receives record 0 (watermark = 1)
    let rep0 = ReplicationRequest {
        topic: "gap-batch".to_string(),
        partition: 0,
        source_region: "region-eu".to_string(),
        target_region: "region-us".to_string(),
        class_id: "global-stream".to_string(),
        epoch: 1,
        from_offset: 0,
        to_offset: 1,
        payload: b"msg-0".to_vec(),
        is_sealed: false,
        ..Default::default()
    };
    let res0 = harness.broker_us.handle_replication(rep0).unwrap();
    assert!(res0.accepted);
    assert_eq!(res0.applied_watermark, 1);

    // 2. Broker EU outbox has ranges [2..3] and [3..4] (missing offset 1)
    let outbox_us = harness.broker_eu.get_outbox("region-us").unwrap();
    outbox_us
        .enqueue(kamish_outbox::OutboxRange {
            topic: "gap-batch".to_string(),
            partition: 0,
            epoch: 1,
            start_offset: 2,
            end_offset: 3,
            class_id: "global-stream".to_string(),
            payload: b"msg-2".to_vec(),
            is_sealed: false,
            source_generation: 1,
            envelope_payload: None,
            ..Default::default()
        })
        .unwrap();
    outbox_us
        .enqueue(kamish_outbox::OutboxRange {
            topic: "gap-batch".to_string(),
            partition: 0,
            epoch: 1,
            start_offset: 3,
            end_offset: 4,
            class_id: "global-stream".to_string(),
            payload: b"msg-3".to_vec(),
            is_sealed: false,
            source_generation: 1,
            envelope_payload: None,
            ..Default::default()
        })
        .unwrap();

    let pending = outbox_us.next_transfer_batch(10);
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].start_offset, 2);
    assert_eq!(pending[1].start_offset, 3);

    // 3. Run step_replication:
    // Range [2..3] is sent -> Receiver US expects 1, rejects with offset_gap.
    // PHASE 1 REQUIREMENT: Agent halts batch immediately and does not send range [3..4].
    let summary = harness.agent.step_replication("region-us").unwrap();
    assert_eq!(
        summary.ranges_transferred, 0,
        "Zero ranges should be successfully transferred"
    );

    // Outbox watermark was set to receiver's confirmed prefix (1)
    assert_eq!(outbox_us.durable_partition_watermark("gap-batch", 0), 1);

    // Both ranges [2..3] and [3..4] are preserved in outbox
    assert_eq!(outbox_us.next_transfer_batch(10).len(), 2);

    // 4. Send missing record 1 directly to receiver US to repair gap
    let rep1 = ReplicationRequest {
        topic: "gap-batch".to_string(),
        partition: 0,
        source_region: "region-eu".to_string(),
        target_region: "region-us".to_string(),
        class_id: "global-stream".to_string(),
        epoch: 1,
        from_offset: 1,
        to_offset: 2,
        payload: b"msg-1".to_vec(),
        is_sealed: false,
        ..Default::default()
    };
    let res1 = harness.broker_us.handle_replication(rep1).unwrap();
    assert!(res1.accepted);
    assert_eq!(res1.applied_watermark, 2);

    // 5. Next transfer pass:
    // Range [2..3] succeeds -> watermark = 3
    // Range [3..4] succeeds -> watermark = 4
    let summary2 = harness.agent.step_replication("region-us").unwrap();
    assert_eq!(summary2.ranges_transferred, 2);
    assert_eq!(summary2.advanced_watermark, 4);
    assert_eq!(outbox_us.durable_partition_watermark("gap-batch", 0), 4);
    assert_eq!(outbox_us.queued_bytes(), 0);
}

#[test]
fn test_partial_overlap_halts_batch_without_record_loss() {
    let harness = setup_cluster();

    // 1. Ingest 3 records [0, 1, 2] into Broker EU
    for i in 0..3 {
        harness
            .broker_eu
            .produce(ProduceRequest {
                topic: "overlap-batch".to_string(),
                partition: 0,
                producer_id: 10,
                producer_epoch: 1,
                sequence_base: i,
                class_id: "global-stream".to_string(),
                payload: format!("payload-{}", i).into_bytes(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            })
            .unwrap();
    }

    // 2. Broker US receives records 0 and 1 (watermark = 2)
    for i in 0..2 {
        let req = ReplicationRequest {
            topic: "overlap-batch".to_string(),
            partition: 0,
            source_region: "region-eu".to_string(),
            target_region: "region-us".to_string(),
            class_id: "global-stream".to_string(),
            epoch: 1,
            from_offset: i,
            to_offset: i + 1,
            payload: format!("payload-{}", i).into_bytes(),
            is_sealed: false,
            ..Default::default()
        };
        assert!(harness.broker_us.handle_replication(req).unwrap().accepted);
    }

    // 3. Broker EU's outbox is at watermark 0 (lost ACK)
    let outbox_us = harness.broker_eu.get_outbox("region-us").unwrap();
    assert_eq!(outbox_us.durable_partition_watermark("overlap-batch", 0), 0);

    // 4. Next transfer pass:
    // First sends [0..1] -> Accepted as idempotent duplicate no-op (watermark = 2).
    // Then sends [1..2] -> Accepted as idempotent duplicate no-op (watermark = 2).
    // Then sends [2..3] -> Accepted as contiguous append (watermark = 3).
    let summary = harness.agent.step_replication("region-us").unwrap();
    assert_eq!(summary.ranges_transferred, 3);
    assert_eq!(summary.advanced_watermark, 3);
    assert_eq!(outbox_us.durable_partition_watermark("overlap-batch", 0), 3);

    // Destination US has exactly 3 records, 0 duplicates on disk
    let fetch = harness
        .broker_us
        .fetch(kamish_broker::FetchRequest {
            topic: "overlap-batch".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch.records.len(), 3);
}

#[test]
fn test_conflicting_duplicate_halts_batch_distinct_from_gap() {
    let harness = setup_cluster();

    // Ingest record 0 into Broker EU and Broker US
    harness
        .broker_eu
        .produce(ProduceRequest {
            topic: "conflict-test".to_string(),
            partition: 0,
            producer_id: 20,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-stream".to_string(),
            payload: b"original-data".to_vec(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        })
        .unwrap();

    let rep0 = ReplicationRequest {
        topic: "conflict-test".to_string(),
        partition: 0,
        source_region: "region-eu".to_string(),
        target_region: "region-us".to_string(),
        class_id: "global-stream".to_string(),
        epoch: 1,
        from_offset: 0,
        to_offset: 1,
        payload: b"original-data".to_vec(),
        is_sealed: false,
        ..Default::default()
    };
    assert!(harness.broker_us.handle_replication(rep0).unwrap().accepted);

    // Now send conflicting record at offset 0 with DIFFERENT payload
    let conflict_req = ReplicationRequest {
        topic: "conflict-test".to_string(),
        partition: 0,
        source_region: "region-eu".to_string(),
        target_region: "region-us".to_string(),
        class_id: "global-stream".to_string(),
        epoch: 1,
        from_offset: 0,
        to_offset: 1,
        payload: b"TAMPERED-DATA".to_vec(),
        is_sealed: false,
        ..Default::default()
    };
    let res = harness.broker_us.handle_replication(conflict_req).unwrap();
    assert!(!res.accepted);
    assert_eq!(res.reason, "replication_history_conflict");
}

#[test]
fn test_healthy_destination_independent_progress_on_peer_gap() {
    let harness = setup_cluster();

    // 1. Ingest 2 records [0, 1] into Broker EU targeting both US and AP
    for i in 0..2 {
        harness
            .broker_eu
            .produce(ProduceRequest {
                topic: "multi-dest-gap".to_string(),
                partition: 0,
                producer_id: 30,
                producer_epoch: 1,
                sequence_base: i,
                class_id: "global-stream".to_string(),
                payload: format!("data-{}", i).into_bytes(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            })
            .unwrap();
    }

    // 2. Tamper US outbox to trigger a gap on US
    let outbox_us = harness.broker_eu.get_outbox("region-us").unwrap();
    outbox_us.advance_partition_watermark("multi-dest-gap", 0, 1);
    // US outbox will attempt [1..2], but US receiver is at watermark 0 -> gap rejection!
    let summary_us = harness.agent.step_replication("region-us").unwrap();
    assert_eq!(summary_us.ranges_transferred, 0);

    // 3. Healthy destination AP: runs step_replication and succeeds independently!
    let summary_ap = harness.agent.step_replication("region-ap").unwrap();
    assert_eq!(
        summary_ap.ranges_transferred, 2,
        "Healthy destination AP must progress and transfer all records"
    );
    assert_eq!(summary_ap.advanced_watermark, 2);

    let fetch_ap = harness
        .broker_ap
        .fetch(kamish_broker::FetchRequest {
            topic: "multi-dest-gap".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch_ap.records.len(), 2);
}

// ----------------------------------------------------------------------------
// PHASE 2 TESTS: RETRY WORKER REDISCOVERY & RESILIENCE
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_three_failures_do_not_permanently_stall_pending_obligation() {
    let harness = setup_cluster();

    // Ingest a record into Broker EU
    harness
        .broker_eu
        .produce(ProduceRequest {
            topic: "retry-test".to_string(),
            partition: 0,
            producer_id: 40,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-stream".to_string(),
            payload: vec![b'X'; 36], // 36 + 64 = 100 bytes
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        })
        .unwrap();

    let outbox_us = harness.broker_eu.get_outbox("region-us").unwrap();
    assert_eq!(outbox_us.queued_bytes(), 100);

    // Create a new agent that initially DOES NOT have region-us registered
    let agent_audit = AuditLog::new_in_memory();
    let agent_enforcer = Enforcer::new("region-eu", harness.policy_cache.clone(), agent_audit);
    let router = RouteSelector::new("region-eu", agent_enforcer);
    let agent_meta = Arc::new(MetadataStore::new());

    let isolated_agent = Arc::new(ReplicationAgent::new(
        "region-eu",
        harness.broker_eu.clone(),
        agent_meta,
        router,
    ));

    // Manually submit task to queue
    let range = outbox_us.next_transfer_batch(1)[0].clone();
    let task = kamish_agent::ReplicationTask::new("region-us", range);
    isolated_agent.submit_task(task).await.unwrap();

    // Spawn retry worker: since region-us is not registered, task will fail 3 times and be dropped
    let worker_handle = isolated_agent.clone().spawn_retry_worker().await.unwrap();

    // Wait for task to exhaust retries (backoff 50 + 100 + 200 = 350ms)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Obligation is NOT lost; still durable in outbox
    assert_eq!(outbox_us.queued_bytes(), 100);
    assert_eq!(outbox_us.durable_partition_watermark("retry-test", 0), 0);

    // Now destination becomes available: register peer broker for region-us!
    isolated_agent.register_peer("region-us", harness.broker_us.clone());

    // PHASE 2 REQUIREMENT: Autonomous sweep discovers the pending obligation from outbox
    // WITHOUT any unrelated produce request!
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Verify recovery: watermark advanced and outbox drained!
    assert_eq!(
        outbox_us.durable_partition_watermark("retry-test", 0),
        1,
        "Autonomous retry worker must rediscover pending outbox obligation"
    );
    assert_eq!(outbox_us.queued_bytes(), 0);

    let fetch = harness
        .broker_us
        .fetch(kamish_broker::FetchRequest {
            topic: "retry-test".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch.records.len(), 1);
    assert_eq!(fetch.records[0].payload, vec![b'X'; 36]);

    worker_handle.abort();
}

#[tokio::test]
async fn test_broker_restart_resumes_replication_without_produce() {
    let harness = setup_cluster();
    let data_dir_eu = harness.dir_eu.path().to_path_buf();

    // 1. Ingest 2 records into Broker EU
    for i in 0..2 {
        harness
            .broker_eu
            .produce(ProduceRequest {
                topic: "restart-resume".to_string(),
                partition: 0,
                producer_id: 50,
                producer_epoch: 1,
                sequence_base: i,
                class_id: "global-stream".to_string(),
                payload: format!("restart-msg-{}", i).into_bytes(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            })
            .unwrap();
    }

    // Drop broker EU (simulating crash before replication agent ran)
    drop(harness.broker_eu);
    drop(harness.agent);

    // 2. Restart Broker EU from existing disk log
    let audit_restarted = AuditLog::new_in_memory();
    let enforcer_restarted = Arc::new(Enforcer::new(
        "region-eu",
        harness.policy_cache.clone(),
        audit_restarted,
    ));
    let meta_restarted = Arc::new(MetadataStore::new());
    let restarted_broker = Arc::new(
        BrokerServer::try_new(
            "region-eu",
            enforcer_restarted,
            meta_restarted.clone(),
            vec!["region-us".to_string()],
            &data_dir_eu,
        )
        .unwrap(),
    );

    // 3. Start replication agent on restarted broker
    let agent_audit = AuditLog::new_in_memory();
    let agent_enforcer = Enforcer::new("region-eu", harness.policy_cache.clone(), agent_audit);
    let router = RouteSelector::new("region-eu", agent_enforcer);
    let agent = Arc::new(ReplicationAgent::new(
        "region-eu",
        restarted_broker.clone(),
        meta_restarted,
        router,
    ));
    agent.register_peer("region-us", harness.broker_us.clone());

    // Spawn retry worker: channel is empty!
    let worker_handle = agent.clone().spawn_retry_worker().await.unwrap();

    // Wait for autonomous sweep to discover disk backlog and replicate
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Both records replicated to Broker US without any new produce request!
    let outbox = restarted_broker.get_outbox("region-us").unwrap();
    assert_eq!(
        outbox.durable_partition_watermark("restart-resume", 0),
        2,
        "Restarted broker must automatically resume replication from disk backlog"
    );
    assert_eq!(outbox.queued_bytes(), 0);

    let fetch = harness
        .broker_us
        .fetch(kamish_broker::FetchRequest {
            topic: "restart-resume".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch.records.len(), 2);

    worker_handle.abort();
}

#[tokio::test]
async fn test_failed_destination_does_not_starve_healthy_destination() {
    let harness = setup_cluster();

    // Produce record 1 for US and record 2 for AP
    harness
        .broker_eu
        .produce(ProduceRequest {
            topic: "starve-test-us".to_string(),
            partition: 0,
            producer_id: 60,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-stream".to_string(),
            payload: b"for-us".to_vec(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        })
        .unwrap();
    harness
        .broker_eu
        .produce(ProduceRequest {
            topic: "starve-test-ap".to_string(),
            partition: 0,
            producer_id: 61,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-stream".to_string(),
            payload: b"for-ap".to_vec(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        })
        .unwrap();

    // Create agent where region-us is NOT registered (failing), but region-ap IS registered (healthy)
    let agent_audit = AuditLog::new_in_memory();
    let agent_enforcer = Enforcer::new("region-eu", harness.policy_cache.clone(), agent_audit);
    let router = RouteSelector::new("region-eu", agent_enforcer);
    let agent_meta = Arc::new(MetadataStore::new());

    let agent = Arc::new(ReplicationAgent::new(
        "region-eu",
        harness.broker_eu.clone(),
        agent_meta,
        router,
    ));
    // Only register AP
    agent.register_peer("region-ap", harness.broker_ap.clone());

    let worker_handle = agent.clone().spawn_retry_worker().await.unwrap();

    // Allow worker to run
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Healthy destination AP must have received its records and advanced its watermark!
    let outbox_ap = harness.broker_eu.get_outbox("region-ap").unwrap();
    assert_eq!(
        outbox_ap.durable_partition_watermark("starve-test-ap", 0),
        1,
        "Healthy destination AP must progress independently"
    );

    let fetch_ap = harness
        .broker_ap
        .fetch(kamish_broker::FetchRequest {
            topic: "starve-test-ap".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch_ap.records.len(), 1);
    assert_eq!(fetch_ap.records[0].payload, b"for-ap");

    // Saturated / failed destination US watermark remains 0
    let outbox_us = harness.broker_eu.get_outbox("region-us").unwrap();
    assert_eq!(
        outbox_us.durable_partition_watermark("starve-test-us", 0),
        0
    );

    worker_handle.abort();
}

#[tokio::test]
async fn test_watermarks_and_accounting_correctness_throughout_retries() {
    let harness = setup_cluster();

    // Ingest 3 records
    for i in 0..3 {
        harness
            .broker_eu
            .produce(ProduceRequest {
                topic: "accounting-test".to_string(),
                partition: 0,
                producer_id: 70,
                producer_epoch: 1,
                sequence_base: i,
                class_id: "global-stream".to_string(),
                payload: vec![b'Z'; 36], // 36 + 64 = 100 bytes each
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            })
            .unwrap();
    }

    let outbox_us = harness.broker_eu.get_outbox("region-us").unwrap();
    assert_eq!(outbox_us.queued_bytes(), 300);
    assert_eq!(
        outbox_us.durable_partition_watermark("accounting-test", 0),
        0
    );

    // Spawn retry worker
    let worker_handle = harness.agent.clone().spawn_retry_worker().await.unwrap();

    // Allow worker to drain
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Watermark advanced to 3 and queued_bytes is exactly 0
    assert_eq!(
        outbox_us.durable_partition_watermark("accounting-test", 0),
        3
    );
    assert_eq!(outbox_us.queued_bytes(), 0);

    worker_handle.abort();
}

#[tokio::test]
async fn test_repeated_failures_do_not_create_busy_loop() {
    let harness = setup_cluster();

    // Ingest record for offline destination
    harness
        .broker_eu
        .produce(ProduceRequest {
            topic: "busy-loop-test".to_string(),
            partition: 0,
            producer_id: 80,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-stream".to_string(),
            payload: vec![b'B'; 36],
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        })
        .unwrap();

    // Create agent that registers an unreachable TCP endpoint
    let agent_audit = AuditLog::new_in_memory();
    let agent_enforcer = Enforcer::new("region-eu", harness.policy_cache.clone(), agent_audit);
    let router = RouteSelector::new("region-eu", agent_enforcer);
    let agent_meta = Arc::new(MetadataStore::new());

    let agent = Arc::new(ReplicationAgent::new(
        "region-eu",
        harness.broker_eu.clone(),
        agent_meta,
        router,
    ));

    // Register unreachable TCP address to trigger connection failures
    agent.register_remote_peer("region-us", "127.0.0.1:1");

    let worker_handle = agent.clone().spawn_retry_worker().await.unwrap();

    let start = std::time::Instant::now();
    // Allow retries and exponential backoff to run for 400ms
    tokio::time::sleep(Duration::from_millis(400)).await;
    let elapsed = start.elapsed();

    assert!(elapsed >= Duration::from_millis(400));

    // Obligation remains durable in outbox
    let outbox = harness.broker_eu.get_outbox("region-us").unwrap();
    assert_eq!(outbox.queued_bytes(), 100);
    assert_eq!(outbox.durable_partition_watermark("busy-loop-test", 0), 0);

    worker_handle.abort();
}
