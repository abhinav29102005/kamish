//! Multi-Region Ownership Admission & Fail-Closed Enforcement Tests
//!
//! Validates:
//! - Multi-region mode strictly fails closed on unregistered streams without silent epoch assumption.
//! - With a registered partition owner and valid lease, produce succeeds at the committed epoch.
//! - Produce on an expired lease is rejected.
//! - Produce on a lease owned by another region is rejected.
//!
//! Ground Rule Notice:
//! Local fail-closed rejection of unregistered streams enforces local admission boundaries;
//! it does NOT constitute distributed authority, consensus verification, or quorum durability.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::{BrokerError, BrokerServer, ProduceRequest};
use kamish_enforce::Enforcer;
use kamish_meta::{DeploymentMode, MetadataStore, ResidencyPolicy};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

fn setup_multiregion_broker(
    local_region: &str,
    peers: Vec<String>,
) -> (Arc<BrokerServer>, Arc<MetadataStore>, tempfile::TempDir) {
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
        policy_id: "pol-multiregion".to_string(),
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

    let enforcer = Arc::new(Enforcer::new(
        local_region,
        Arc::new(RwLock::new(cache)),
        AuditLog::new_in_memory(),
    ));

    let temp_dir = tempfile::tempdir().unwrap();
    let meta = Arc::new(MetadataStore::new_multi_region());

    let broker = Arc::new(BrokerServer::new(
        local_region,
        enforcer,
        meta.clone(),
        peers,
        temp_dir.path(),
    ));

    (broker, meta, temp_dir)
}

#[test]
fn test_multiregion_mode_unregistered_stream_fails_closed_and_registered_succeeds() {
    let (broker, meta, _dir) = setup_multiregion_broker("region-a", vec!["region-b".into()]);
    assert_eq!(broker.meta.deployment_mode(), DeploymentMode::MultiRegion);

    let produce_req = ProduceRequest {
        topic: "events".to_string(),
        partition: 0,
        class_id: "general".to_string(),
        payload: b"multi-region payload".to_vec(),
        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        producer_id: 101,
        producer_epoch: 1,
        sequence_base: 0,
    };

    // 1. Unregistered stream MUST fail closed in MultiRegion mode
    let unreg_err = broker.produce(produce_req.clone()).unwrap_err();
    assert!(
        matches!(unreg_err, BrokerError::PolicyDenied(ref reason) if reason == "unregistered_stream_in_multi_region_mode"),
        "Unregistered stream must be rejected with policy denial in multi-region mode, got: {:?}",
        unreg_err
    );

    // 2. Authoritatively register ownership and lease for this partition
    meta.set_ownership("events", 0, "region-a", 1, 60_000)
        .expect("registration of partition ownership must succeed");

    // 3. With registered ownership, produce MUST succeed at committed epoch 1
    let success = broker
        .produce(produce_req.clone())
        .expect("registered partition produce must succeed");
    assert!(success.sovereignty_allowed);
    assert_eq!(success.committed_epoch, 1);
    assert_eq!(success.base_offset, 0);

    // 4. If partition owner is another region, produce MUST fail closed
    meta.set_ownership("events", 0, "region-b", 2, 60_000)
        .unwrap();
    let wrong_owner_err = broker
        .produce(ProduceRequest {
            sequence_base: 1,
            ..produce_req.clone()
        })
        .unwrap_err();
    assert!(
        matches!(wrong_owner_err, BrokerError::PolicyDenied(ref reason) if reason == "not_current_partition_owner"),
        "Produce on partition owned by another region must fail closed, got: {:?}",
        wrong_owner_err
    );

    // 5. If lease has expired, produce MUST fail closed
    meta.set_ownership("events", 0, "region-a", 3, -1000)
        .unwrap(); // expired 1s ago
    let expired_err = broker
        .produce(ProduceRequest {
            sequence_base: 2,
            ..produce_req
        })
        .unwrap_err();
    assert!(
        matches!(expired_err, BrokerError::PolicyDenied(ref reason) if reason == "not_current_partition_owner"),
        "Produce on expired lease must fail closed, got: {:?}",
        expired_err
    );
}
