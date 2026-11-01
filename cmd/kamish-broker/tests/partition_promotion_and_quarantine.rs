//! Partition Failover Promotion & Quarantine Routing Tests (FN-02, §3.1.3, §3.1.4, W20)
//!
//! Validates:
//! 1. Candidate ineligibility under E8 policy rejects promotion.
//! 2. Unexpired prior owner lease rejects promotion with clock drift buffer.
//! 3. Insufficient replica watermark rejects promotion (INV-3).
//! 4. Legitimate failover promotion commits epoch + 1 via atomic CAS and enables produce at new epoch.
//! 5. Superseded old owner is fenced from producing (`not_current_partition_owner`).
//! 6. Divergent uncommitted log suffix on superseded owner is cleanly truncated and isolated into QuarantineStore.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_broker::{BrokerError, BrokerServer, FetchRequest, ProduceRequest};
use kamish_enforce::Enforcer;
use kamish_meta::{MetadataAuthority, MetadataStore};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tempfile::tempdir;

fn setup_two_region_cluster() -> (
    Arc<BrokerServer>,
    Arc<BrokerServer>,
    tempfile::TempDir,
    tempfile::TempDir,
    Arc<MetadataStore>,
) {
    let meta = Arc::new(MetadataStore::new_multi_region());

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let mut legal = HashSet::new();
    legal.insert("region-a".to_string());
    legal.insert("region-b".to_string());

    let mut modes = HashMap::new();
    modes.insert("region-a".to_string(), DestinationMode::Cleartext);
    modes.insert("region-b".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    // Class "general": allows promotion to region-b
    classes.insert(
        "general".to_string(),
        ClassRule {
            class_id: "general".to_string(),
            description: "Promotable General Topic".to_string(),
            legal_regions: legal.clone(),
            destination_modes: modes.clone(),
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-a".to_string(), "region-b".to_string()],
            transit_allowed: true,
        },
    );
    // Class "restricted-a": only region-a can own/promote
    classes.insert(
        "restricted-a".to_string(),
        ClassRule {
            class_id: "restricted-a".to_string(),
            description: "Non-failover Topic".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-a".to_string()],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-cluster".to_string(),
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

    let dir_a = tempdir().unwrap();
    let audit_a =
        kamish_audit::AuditLog::open(dir_a.path().join("audit.log").to_str().unwrap()).unwrap();
    let mut cache_a = PolicyCache::new(trusted.clone());
    cache_a.update(signed.clone(), Utc::now().timestamp()).unwrap();
    let enforcer_a = Arc::new(Enforcer::new(
        "region-a",
        Arc::new(RwLock::new(cache_a)),
        audit_a,
    ));

    let broker_a = Arc::new(BrokerServer::new(
        "region-a",
        enforcer_a,
        meta.clone(),
        vec!["region-b".into()],
        dir_a.path(),
    ).with_promotion_safety_margin_ms(200));

    let dir_b = tempdir().unwrap();
    let audit_b =
        kamish_audit::AuditLog::open(dir_b.path().join("audit.log").to_str().unwrap()).unwrap();
    let mut cache_b = PolicyCache::new(trusted);
    cache_b.update(signed, Utc::now().timestamp()).unwrap();
    let enforcer_b = Arc::new(Enforcer::new(
        "region-b",
        Arc::new(RwLock::new(cache_b)),
        audit_b,
    ));

    let broker_b = Arc::new(BrokerServer::new(
        "region-b",
        enforcer_b,
        meta.clone(),
        vec!["region-a".into()],
        dir_b.path(),
    ).with_promotion_safety_margin_ms(200));

    (broker_a, broker_b, dir_a, dir_b, meta)
}

#[tokio::test]
async fn test_promotion_rejects_ineligible_candidate() {
    let (broker_a, broker_b, _dir_a, _dir_b, _meta) = setup_two_region_cluster();

    // 1. Region A claims ownership of "restricted-a" at epoch 1 with 100ms lease
    broker_a
        .claim_partition_ownership("restricted-a", 0, 0, 1, 100)
        .unwrap();

    // 2. Wait for lease + clock drift buffer to pass
    tokio::time::sleep(Duration::from_millis(350)).await;

    // 3. Region B attempts promotion on "restricted-a" -> Policy E8 denies because Region B is not in failover_candidates
    let err = broker_b
        .promote_partition("restricted-a", 0, Some("restricted-a"), None)
        .unwrap_err();

    assert!(
        matches!(err, BrokerError::PolicyDenied(ref s) if s.contains("promotion_denied_by_policy")),
        "Expected policy denial for ineligible candidate, got: {:?}",
        err
    );
}

#[tokio::test]
async fn test_promotion_rejects_unexpired_lease() {
    let (broker_a, broker_b, _dir_a, _dir_b, _meta) = setup_two_region_cluster();

    // 1. Region A claims ownership with 5000ms lease
    broker_a
        .claim_partition_ownership("active-lease-topic", 0, 0, 1, 5000)
        .unwrap();

    // 2. Region B attempts immediate promotion -> rejected because prior owner lease is active
    let err = broker_b
        .promote_partition("active-lease-topic", 0, Some("general"), None)
        .unwrap_err();

    assert!(
        matches!(err, BrokerError::PolicyDenied(ref s) if s.contains("prior_owner_lease_active")),
        "Expected lease active rejection, got: {:?}",
        err
    );
}

#[tokio::test]
async fn test_promotion_rejects_behind_replica_watermark() {
    let (broker_a, broker_b, _dir_a, _dir_b, _meta) = setup_two_region_cluster();

    // 1. Region A claims partition at epoch 1 with 100ms lease
    broker_a
        .claim_partition_ownership("watermark-topic", 0, 0, 1, 100)
        .unwrap();

    // Region A produces 3 records
    for seq in 0..3 {
        broker_a
            .produce(ProduceRequest {
                topic: "watermark-topic".to_string(),
                partition: 0,
                producer_id: 201,
                producer_epoch: 1,
                sequence_base: seq,
                class_id: "general".to_string(),
                payload: format!("record-{}", seq).into_bytes(),
                residency_rule: None,
            })
            .unwrap();
    }

    // 2. Wait for lease to expire
    tokio::time::sleep(Duration::from_millis(350)).await;

    // 3. Region B attempts promotion requiring watermark 3, but Region B has 0 records locally
    let err = broker_b
        .promote_partition("watermark-topic", 0, Some("general"), Some(3))
        .unwrap_err();

    assert!(
        matches!(err, BrokerError::PolicyDenied(ref s) if s.contains("candidate_watermark_behind")),
        "Expected candidate_watermark_behind error, got: {:?}",
        err
    );
}

#[tokio::test]
async fn test_promotion_success_fences_old_owner_and_quarantines_divergent_suffix() {
    let (broker_a, broker_b, _dir_a, _dir_b, _meta) = setup_two_region_cluster();

    // 1. Region A claims partition at epoch 1 with 150ms lease
    broker_a
        .claim_partition_ownership("failover-topic", 0, 0, 1, 150)
        .unwrap();

    // Region A writes 2 committed records (offset 0 and 1)
    for seq in 0..2 {
        broker_a
            .produce(ProduceRequest {
                topic: "failover-topic".to_string(),
                partition: 0,
                producer_id: 301,
                producer_epoch: 1,
                sequence_base: seq,
                class_id: "general".to_string(),
                payload: format!("committed-{}", seq).into_bytes(),
                residency_rule: None,
            })
            .unwrap();
    }

    // Simulate replication to Region B: replicate record 0 and 1
    for offset in 0..2 {
        let rec = broker_a
            .fetch(FetchRequest {
                topic: "failover-topic".to_string(),
                partition: 0,
                class_id: "general".to_string(),
                start_offset: offset,
                max_records: 1,
            })
            .unwrap()
            .records[0]
            .clone();
        broker_b
            .handle_replication(kamish_broker::ReplicationRequest {
                topic: "failover-topic".to_string(),
                partition: 0,
                source_region: "region-a".to_string(),
                target_region: "region-b".to_string(),
                class_id: "general".to_string(),
                epoch: 1,
                from_offset: offset,
                to_offset: offset + 1,
                payload: rec.payload,
                is_sealed: false,
                context: rec.context,
                policy_version: rec.policy_version,
                checksum: Some(rec.checksum),
                ..Default::default()
            })
            .unwrap();
    }
    assert_eq!(
        broker_b
            .fetch(FetchRequest {
                topic: "failover-topic".to_string(),
                partition: 0,
                class_id: "general".to_string(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap()
            .high_watermark,
        2
    );

    // 2. Region A becomes isolated / slow and writes 2 uncommitted speculative records (offset 2 and 3)
    for seq in 2..4 {
        broker_a
            .produce(ProduceRequest {
                topic: "failover-topic".to_string(),
                partition: 0,
                producer_id: 301,
                producer_epoch: 1,
                sequence_base: seq,
                class_id: "general".to_string(),
                payload: format!("uncommitted-speculative-{}", seq).into_bytes(),
                residency_rule: None,
            })
            .unwrap();
    }
    assert_eq!(
        broker_a
            .fetch(FetchRequest {
                topic: "failover-topic".to_string(),
                partition: 0,
                class_id: "general".to_string(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap()
            .high_watermark,
        4
    );

    // 3. Region A's lease expires
    tokio::time::sleep(Duration::from_millis(380)).await;

    // 4. Region B detects outage and promotes partition with required watermark 2
    let promoted_ownership = broker_b
        .promote_partition("failover-topic", 0, Some("general"), Some(2))
        .expect("Promotion of Region B must succeed");

    assert_eq!(promoted_ownership.epoch, 2);
    assert_eq!(promoted_ownership.owner_region, "region-b");
    assert!(broker_b.is_partition_renewal_active("failover-topic", 0));

    // 5. Region B (new leader) can now produce records under epoch 2
    let resp_b = broker_b
        .produce(ProduceRequest {
            topic: "failover-topic".to_string(),
            partition: 0,
            producer_id: 401,
            producer_epoch: 2,
            sequence_base: 0,
            class_id: "general".to_string(),
            payload: b"new-leader-record-epoch2".to_vec(),
            residency_rule: None,
        })
        .expect("New leader produce must succeed");
    assert_eq!(resp_b.committed_epoch, 2);
    assert_eq!(resp_b.base_offset, 2);

    // 6. Region A reconnects: attempts to produce -> FENCED!
    let err_a = broker_a
        .produce(ProduceRequest {
            topic: "failover-topic".to_string(),
            partition: 0,
            producer_id: 301,
            producer_epoch: 1,
            sequence_base: 4,
            class_id: "general".to_string(),
            payload: b"stale-producer-write".to_vec(),
            residency_rule: None,
        })
        .unwrap_err();

    assert!(
        matches!(err_a, BrokerError::PolicyDenied(ref s) if s == "not_current_partition_owner"),
        "Superseded Region A must be fenced from producing, got: {:?}",
        err_a
    );

    // 7. Region A reconciles divergent log against agreed watermark 2:
    // It must quarantine its 2 speculative records (offset 2 and 3)
    let quarantined_count = broker_a
        .quarantine_divergent_suffix("failover-topic", 0, 2)
        .expect("Quarantine reconciliation must succeed");

    assert_eq!(quarantined_count, 2, "Expected 2 divergent records to be quarantined");

    // Verify Region A's active log high watermark is truncated back to 2
    let fetch_a = broker_a
        .fetch(FetchRequest {
            topic: "failover-topic".to_string(),
            partition: 0,
            class_id: "general".to_string(),
            start_offset: 0,
            max_records: 10,
        })
        .unwrap();
    assert_eq!(fetch_a.high_watermark, 2);
    assert_eq!(fetch_a.records.len(), 2);
    assert_eq!(fetch_a.records[0].payload, b"committed-0");
    assert_eq!(fetch_a.records[1].payload, b"committed-1");

    // Verify QuarantineStore holds the 2 divergent records
    assert_eq!(broker_a.quarantine().count(), 2);

    // Repeating quarantine is an idempotent no-op
    let repeat = broker_a.quarantine_divergent_suffix("failover-topic", 0, 2).unwrap();
    assert_eq!(repeat, 0);
}

#[test]
fn test_partition_truncation_failure_safety_and_restart() {
    let dir = tempdir().unwrap();
    let log = kamish_log::PartitionLog::open(dir.path(), 1).unwrap();

    // Append 5 records (offset 0..5)
    for i in 0..5 {
        log.append(1, "general", 1, format!("msg-{}", i).into_bytes()).unwrap();
    }
    assert_eq!(log.next_offset(), 5);
    assert_eq!(log.high_watermark(), 5);

    // 1. read_suffix(2) safely returns 3 records without mutating log
    let suffix = log.read_suffix(2);
    assert_eq!(suffix.len(), 3);
    assert_eq!(log.next_offset(), 5);
    assert_eq!(log.high_watermark(), 5);

    // 2. Truncation to future or same offset is an idempotent no-op
    let noop = log.truncate_to_offset(5).unwrap();
    assert!(noop.is_empty());
    assert_eq!(log.next_offset(), 5);

    // 3. Successful truncation to offset 2
    let superseded = log.truncate_to_offset(2).unwrap();
    assert_eq!(superseded.len(), 3);
    assert_eq!(superseded[0].payload, b"msg-2");
    assert_eq!(superseded[1].payload, b"msg-3");
    assert_eq!(superseded[2].payload, b"msg-4");
    assert_eq!(log.next_offset(), 2);
    assert_eq!(log.high_watermark(), 2);

    // 4. Restart verification: drop old instance and reopen PartitionLog from disk
    drop(log);
    let reopened = kamish_log::PartitionLog::open(dir.path(), 1).unwrap();
    assert_eq!(reopened.next_offset(), 2);
    assert_eq!(reopened.high_watermark(), 2);
    let records = reopened.read(0, 10);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].payload, b"msg-0");
    assert_eq!(records[1].payload, b"msg-1");
}

#[test]
fn test_quarantine_store_persistence_recovery_and_idempotent_retry() {
    let dir = tempdir().unwrap();
    let q_store = kamish_own::QuarantineStore::open(dir.path()).unwrap();

    let record1 = kamish_log::RecordEnvelope::new(1, "general", 1, 10, b"divergent-1".to_vec());
    let record2 = kamish_log::RecordEnvelope::new(1, "general", 1, 11, b"divergent-2".to_vec());
    let batch = vec![record1, record2];

    // 1. Initial quarantine persistence
    q_store.quarantine_records(batch.clone()).unwrap();
    assert_eq!(q_store.count(), 2);

    // 2. Idempotent retry: persisting the exact same batch does not duplicate records
    q_store.quarantine_records(batch.clone()).unwrap();
    assert_eq!(q_store.count(), 2, "Idempotent retry must not duplicate records");

    // 3. Adding a new unique record increases count by 1
    let record3 = kamish_log::RecordEnvelope::new(2, "general", 1, 12, b"divergent-3".to_vec());
    q_store.quarantine_records(vec![record3]).unwrap();
    assert_eq!(q_store.count(), 3);

    // 4. Restart recovery: reopen QuarantineStore from disk
    let reopened = kamish_own::QuarantineStore::open(dir.path()).unwrap();
    assert_eq!(reopened.count(), 3);
    let records = reopened.records();
    assert_eq!(records[0].payload, b"divergent-1");
    assert_eq!(records[1].payload, b"divergent-2");
    assert_eq!(records[2].payload, b"divergent-3");
}

#[tokio::test]
async fn test_promotion_cas_conflict_and_ambiguity_handling() {
    let (broker_a, broker_b, _dir_a, _dir_b, meta) = setup_two_region_cluster();

    // 1. Region A claims partition at epoch 1 with 100ms lease
    broker_a
        .claim_partition_ownership("cas-conflict-topic", 0, 0, 1, 100)
        .unwrap();

    // Lease expires
    tokio::time::sleep(Duration::from_millis(350)).await;

    // 2. Concurrently, another broker or authority increments epoch to 2
    let stream = kamish_meta::StreamId::new("cas-conflict-topic", 0);
    meta.compare_and_swap_epoch(&stream, 1, 2, "region-c", 10_000).unwrap();

    // 3. Region B attempts promotion: current_ownership in candidate view had epoch 1
    // But CAS on metadata authority will detect epoch conflict (now at epoch 2)
    let err = broker_b
        .promote_partition("cas-conflict-topic", 0, Some("general"), None)
        .unwrap_err();

    // Broker B must be rejected with EpochFenced or conflict, and NOT become owner
    assert!(
        matches!(err, BrokerError::EpochFenced { current: 2, .. } | BrokerError::PolicyDenied(_)),
        "Expected CAS conflict or epoch fenced error, got: {:?}",
        err
    );
    assert!(!broker_b.is_partition_renewal_active("cas-conflict-topic", 0));
}

#[tokio::test]
async fn test_promotion_reconciliation_ordering_isolates_suffix_before_cas() {
    let (broker_a, broker_b, _dir_a, _dir_b, _meta) = setup_two_region_cluster();

    // 1. Region A claims partition at epoch 1 with 150ms lease and writes 2 records
    broker_a.claim_partition_ownership("order-topic", 0, 0, 1, 150).unwrap();
    for seq in 0..2 {
        broker_a.produce(ProduceRequest {
            topic: "order-topic".to_string(),
            partition: 0,
            producer_id: 501,
            producer_epoch: 1,
            sequence_base: seq,
            class_id: "general".to_string(),
            payload: format!("c-{}", seq).into_bytes(),
            residency_rule: None,
        }).unwrap();
    }

    // Replicate to Region B
    for offset in 0..2 {
        let rec = broker_a.fetch(FetchRequest {
            topic: "order-topic".to_string(),
            partition: 0,
            class_id: "general".to_string(),
            start_offset: offset,
            max_records: 1,
        }).unwrap().records[0].clone();
        broker_b.handle_replication(kamish_broker::ReplicationRequest {
            topic: "order-topic".to_string(),
            partition: 0,
            source_region: "region-a".to_string(),
            target_region: "region-b".to_string(),
            class_id: "general".to_string(),
            epoch: 1,
            from_offset: offset,
            to_offset: offset + 1,
            payload: rec.payload,
            is_sealed: false,
            context: rec.context,
            policy_version: rec.policy_version,
            checksum: Some(rec.checksum),
            ..Default::default()
        }).unwrap();
    }

    // Candidate Region B locally receives 1 uncommitted speculative record directly
    let log_b = broker_b.get_or_create_log("order-topic", 0, 1).unwrap();
    log_b.append(1, "general", 1, b"speculative-b".to_vec()).unwrap();
    assert_eq!(log_b.next_offset(), 3);

    // Lease expires
    tokio::time::sleep(Duration::from_millis(380)).await;

    // Promotion with required watermark 2 must isolate speculative record 2 to quarantine
    // and truncate log to 2 before CAS commits epoch 2
    let promoted = broker_b.promote_partition("order-topic", 0, Some("general"), Some(2)).unwrap();
    assert_eq!(promoted.epoch, 2);
    assert_eq!(promoted.owner_region, "region-b");

    // Local log next_offset and high watermark are now exactly 2
    assert_eq!(log_b.next_offset(), 2);
    assert_eq!(log_b.high_watermark(), 2);

    // QuarantineStore holds the speculative record
    assert_eq!(broker_b.quarantine().count(), 1);
    assert_eq!(broker_b.quarantine().records()[0].payload, b"speculative-b");
}

#[tokio::test]
async fn test_restart_preserves_reconciled_state_without_resurrecting_divergence() {
    let dir = tempdir().unwrap();
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let mut legal = HashSet::new();
    legal.insert("region-a".to_string());
    let mut modes = HashMap::new();
    modes.insert("region-a".to_string(), DestinationMode::Cleartext);
    let mut classes = HashMap::new();
    classes.insert("general".to_string(), ClassRule {
        class_id: "general".to_string(),
        description: "General".to_string(),
        legal_regions: legal.clone(),
        destination_modes: modes.clone(),
        ack_mode: AckMode::LocalMajority,
        failover_candidates: vec!["region-a".to_string()],
        transit_allowed: true,
    });
    let policy_doc = PolicyDocument {
        policy_id: "pol-cluster".to_string(),
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

    let audit = kamish_audit::AuditLog::open(dir.path().join("audit.log").to_str().unwrap()).unwrap();
    let mut cache = PolicyCache::new(trusted.clone());
    cache.update(signed.clone(), Utc::now().timestamp()).unwrap();
    let enforcer = Arc::new(Enforcer::new("region-a", Arc::new(RwLock::new(cache)), audit));
    let meta = Arc::new(MetadataStore::new_multi_region());

    let broker = Arc::new(BrokerServer::new(
        "region-a",
        enforcer.clone(),
        meta.clone(),
        vec![],
        dir.path(),
    ));

    // Append 4 records directly to log
    let log = broker.get_or_create_log("restart-topic", 0, 1).unwrap();
    for i in 0..4 {
        log.append(1, "general", 1, format!("rec-{}", i).into_bytes()).unwrap();
    }
    assert_eq!(log.next_offset(), 4);

    // Reconcile divergent suffix back to 2
    let count = broker.quarantine_divergent_suffix("restart-topic", 0, 2).unwrap();
    assert_eq!(count, 2);
    assert_eq!(broker.quarantine().count(), 2);
    assert_eq!(log.next_offset(), 2);

    // Simulate broker process crash and restart: reopen BrokerServer on the same data directory
    drop(log);
    drop(broker);
    let audit_reopened = kamish_audit::AuditLog::open(dir.path().join("audit.log").to_str().unwrap()).unwrap();
    let mut cache_reopened = PolicyCache::new(trusted);
    cache_reopened.update(signed, Utc::now().timestamp()).unwrap();
    let enforcer_reopened = Arc::new(Enforcer::new("region-a", Arc::new(RwLock::new(cache_reopened)), audit_reopened));
    let restarted_broker = BrokerServer::new(
        "region-a",
        enforcer_reopened,
        meta,
        vec![],
        dir.path(),
    );

    // Verify restarted broker log is still at 2
    let restarted_log = restarted_broker.get_or_create_log("restart-topic", 0, 1).unwrap();
    assert_eq!(restarted_log.next_offset(), 2);
    assert_eq!(restarted_log.high_watermark(), 2);
    let recs = restarted_log.read(0, 10);
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0].payload, b"rec-0");
    assert_eq!(recs[1].payload, b"rec-1");

    // Verify quarantine store holds the 2 records across restart
    assert_eq!(restarted_broker.quarantine().count(), 2);
}
