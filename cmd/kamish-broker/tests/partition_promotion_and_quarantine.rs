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
use kamish_meta::MetadataStore;
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
    ));

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
    ));

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
