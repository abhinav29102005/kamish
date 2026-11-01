use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_broker::{BrokerError, BrokerServer, ProduceRequest};
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

fn setup_test_broker(
    local_region: &str,
    meta: Arc<MetadataStore>,
) -> (Arc<BrokerServer>, tempfile::TempDir) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let mut legal = HashSet::new();
    legal.insert(local_region.to_string());
    legal.insert("region-b".to_string());

    let mut modes = HashMap::new();
    modes.insert(local_region.to_string(), DestinationMode::Cleartext);
    modes.insert("region-b".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "general".to_string(),
        ClassRule {
            class_id: "general".to_string(),
            description: "General test class".to_string(),
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

    let dir = tempdir().unwrap();
    let audit_log =
        kamish_audit::AuditLog::open(dir.path().join("audit.log").to_str().unwrap()).unwrap();
    let enforcer = Arc::new(Enforcer::new(
        local_region,
        Arc::new(RwLock::new(cache)),
        audit_log,
    ));

    let broker = Arc::new(BrokerServer::new(
        local_region,
        enforcer,
        meta,
        vec!["region-b".into()],
        dir.path(),
    ));

    (broker, dir)
}

#[tokio::test]
async fn test_proactive_lease_renewal_worker_keeps_lease_alive_across_intervals() {
    let meta = Arc::new(MetadataStore::new_multi_region());
    let (broker, _dir) = setup_test_broker("region-a", meta.clone());

    // 1. Claim partition with short 200ms lease
    let claim = broker
        .claim_partition_ownership("renewal-topic", 0, 0, 1, 200)
        .expect("Initial claim must succeed");
    assert_eq!(claim.epoch, 1);
    assert!(broker.is_partition_renewal_active("renewal-topic", 0));

    // Initial produce succeeds
    let req = ProduceRequest {
        topic: "renewal-topic".to_string(),
        partition: 0,
        producer_id: 101,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "general".to_string(),
        payload: b"initial-record".to_vec(),
        residency_rule: None,
    };
    let resp1 = broker.produce(req.clone()).unwrap();
    assert_eq!(resp1.committed_epoch, 1);

    // 2. Start lease renewal worker with 50ms interval, 50ms timeout, 200ms TTL
    let worker_handle = broker.start_lease_renewal_worker(
        Duration::from_millis(50),
        Duration::from_millis(50),
        200,
    );

    // 3. Sleep 350ms (well past the initial 200ms lease)
    tokio::time::sleep(Duration::from_millis(350)).await;

    // 4. Produce should still succeed because worker proactively renewed the lease!
    let req2 = ProduceRequest {
        sequence_base: 1,
        payload: b"second-record".to_vec(),
        ..req.clone()
    };
    let resp2 = broker
        .produce(req2.clone())
        .expect("Produce after 350ms must succeed due to active background renewal");
    assert_eq!(resp2.committed_epoch, 1);

    // 5. Stop the renewal worker
    broker.stop_lease_renewal();
    let _ = worker_handle.await;

    // 6. Sleep 250ms past the last renewal's 200ms expiration
    tokio::time::sleep(Duration::from_millis(250)).await;

    // 7. Write must now fail closed because lease expired
    let req3 = ProduceRequest {
        sequence_base: 2,
        payload: b"third-record".to_vec(),
        ..req
    };
    let err = broker.produce(req3).unwrap_err();
    assert!(
        matches!(err, BrokerError::PolicyDenied(ref s) if s == "not_current_partition_owner"),
        "Produce after stopping renewal worker must fail closed, got: {:?}",
        err
    );
}

#[tokio::test]
async fn test_lease_renewal_worker_evicts_fenced_partition() {
    let meta = Arc::new(MetadataStore::new_multi_region());
    let (broker_a, _dir_a) = setup_test_broker("region-a", meta.clone());

    // 1. Broker A claims partition 0
    broker_a
        .claim_partition_ownership("eviction-topic", 0, 0, 1, 150)
        .unwrap();
    assert!(broker_a.is_partition_renewal_active("eviction-topic", 0));

    // 2. Stop Broker A renewal (simulating network partition or freeze), let lease expire
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 3. Region B now legitimately claims partition at epoch 2
    let stream_id = kamish_meta::StreamId::new("eviction-topic", 0);
    meta.compare_and_swap_epoch(&stream_id, 1, 2, "region-b", 5000)
        .unwrap();

    // 4. Broker A wakes up and starts renewal worker
    let worker_handle = broker_a.start_lease_renewal_worker(
        Duration::from_millis(40),
        Duration::from_millis(40),
        200,
    );

    // 5. Wait for Broker A's renewal worker to attempt renewal and encounter conflict
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 6. Broker A must have evicted the partition from active renewals
    assert!(
        !broker_a.is_partition_renewal_active("eviction-topic", 0),
        "Superseded partition must be automatically evicted from renewal set"
    );

    broker_a.stop_lease_renewal();
    let _ = worker_handle.await;
}
