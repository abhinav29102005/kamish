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
use kamish_meta::{DeploymentMode, MetadataAuthority, MetadataStore, ResidencyPolicy};
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

    // 2. In MultiRegion mode, direct set_ownership MUST fail closed
    let direct_set_err = meta
        .set_ownership("events", 0, "region-a", 1, 60_000)
        .unwrap_err();
    assert!(
        matches!(direct_set_err, kamish_meta::MetaError::AuthorityError(ref msg) if msg.contains("MultiRegion mode cannot set ownership except through compare_and_swap_epoch")),
        "Direct set_ownership in MultiRegion mode must fail closed, got: {:?}",
        direct_set_err
    );

    // 3. Authoritatively register ownership and lease via compare_and_swap_epoch
    let stream_id = kamish_meta::StreamId::new("events", 0);
    meta.compare_and_swap_epoch(&stream_id, 0, 1, "region-a", 60_000)
        .expect("CAS registration of partition ownership must succeed");

    // 4. With registered ownership, produce MUST succeed at committed epoch 1
    let success = broker
        .produce(produce_req.clone())
        .expect("registered partition produce must succeed");
    assert!(success.sovereignty_allowed);
    assert_eq!(success.committed_epoch, 1);
    assert_eq!(success.base_offset, 0);

    // 5. Transfer to region-b while region-a lease is active MUST fail with LeaseConflict
    let conflict = meta
        .compare_and_swap_epoch(&stream_id, 1, 2, "region-b", 60_000)
        .unwrap_err();
    assert!(
        matches!(conflict, kamish_meta::MetaError::LeaseConflict { .. }),
        "Transfer while prior lease is active must be rejected with LeaseConflict"
    );

    // Revoking prior owner lease allows transfer to region-b
    meta.revoke_lease(&stream_id, 0).unwrap();
    meta.compare_and_swap_epoch(&stream_id, 1, 2, "region-b", 60_000)
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

    // 6. Transfer back to region-a while region-b lease is active MUST fail with LeaseConflict
    let conflict_b = meta
        .compare_and_swap_epoch(&stream_id, 2, 3, "region-a", -1000)
        .unwrap_err();
    assert!(
        matches!(conflict_b, kamish_meta::MetaError::LeaseConflict { .. }),
        "Transfer while prior lease is active must be rejected with LeaseConflict"
    );

    // Revoking prior owner lease allows acquisition of epoch 3 with expired lease
    meta.revoke_lease(&stream_id, 0).unwrap();
    meta.compare_and_swap_epoch(&stream_id, 2, 3, "region-a", -1000)
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

/// Mock metadata authority that simulates a lease expiring right during disk sync
#[derive(Clone)]
struct ExpiringDuringAppendMeta {
    inner: Arc<MetadataStore>,
    call_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl kamish_audit::AuditWitnessAuthority for ExpiringDuringAppendMeta {
    fn commit_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
        sequence: u64,
        tail_hash: &str,
        record_count: u64,
        signature_hex: &str,
        timestamp_ns: i64,
    ) -> Result<(), kamish_audit::AuditError> {
        self.inner.commit_checkpoint(
            stream_topic,
            stream_partition,
            sequence,
            tail_hash,
            record_count,
            signature_hex,
            timestamp_ns,
        )
    }

    fn read_latest_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
    ) -> Result<Option<kamish_audit::AuditWitnessCheckpointData>, kamish_audit::AuditError> {
        self.inner
            .read_latest_checkpoint(stream_topic, stream_partition)
    }
}

impl MetadataAuthority for ExpiringDuringAppendMeta {
    fn deployment_mode(&self) -> DeploymentMode {
        self.inner.deployment_mode()
    }

    fn compare_and_swap_epoch(
        &self,
        stream: &kamish_meta::StreamId,
        expected_epoch: u64,
        new_epoch: u64,
        new_owner_region: &str,
        lease_duration_ms: i64,
    ) -> Result<kamish_meta::PartitionOwnership, kamish_meta::MetaError> {
        self.inner.compare_and_swap_epoch(
            stream,
            expected_epoch,
            new_epoch,
            new_owner_region,
            lease_duration_ms,
        )
    }

    fn grant_lease(
        &self,
        stream: &kamish_meta::StreamId,
        holder_region: &str,
        duration_ms: i64,
    ) -> Result<kamish_meta::LeaseGrant, kamish_meta::MetaError> {
        self.inner.grant_lease(stream, holder_region, duration_ms)
    }

    fn renew_lease(
        &self,
        stream: &kamish_meta::StreamId,
        lease_id: u64,
        duration_ms: i64,
    ) -> Result<kamish_meta::LeaseGrant, kamish_meta::MetaError> {
        self.inner.renew_lease(stream, lease_id, duration_ms)
    }

    fn revoke_lease(
        &self,
        stream: &kamish_meta::StreamId,
        lease_id: u64,
    ) -> Result<kamish_meta::RevocationAck, kamish_meta::MetaError> {
        self.inner.revoke_lease(stream, lease_id)
    }

    fn get_policy_floor(&self, cluster_id: &str) -> Result<u64, kamish_meta::MetaError> {
        self.inner.get_policy_floor(cluster_id)
    }

    fn advance_policy_floor(
        &self,
        cluster_id: &str,
        new_floor: u64,
    ) -> Result<u64, kamish_meta::MetaError> {
        self.inner.advance_policy_floor(cluster_id, new_floor)
    }

    fn issue_source_identity(
        &self,
        cluster_id: &str,
        node_id: &str,
    ) -> Result<kamish_meta::SourceIdentityGrant, kamish_meta::MetaError> {
        self.inner.issue_source_identity(cluster_id, node_id)
    }

    fn commit_audit_checkpoint(
        &self,
        checkpoint: kamish_meta::AuditCheckpoint,
    ) -> Result<kamish_meta::CheckpointReceipt, kamish_meta::MetaError> {
        self.inner.commit_audit_checkpoint(checkpoint)
    }

    fn read_audit_checkpoint(
        &self,
        stream: &kamish_meta::StreamId,
    ) -> Result<Option<kamish_meta::AuditCheckpoint>, kamish_meta::MetaError> {
        self.inner.read_audit_checkpoint(stream)
    }

    fn get_ownership(
        &self,
        topic: &str,
        partition: u32,
    ) -> Option<kamish_meta::PartitionOwnership> {
        let count = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count == 0 {
            // First call (pre-append check): return valid ownership
            self.inner.get_ownership(topic, partition)
        } else {
            // Subsequent call (post-append check): simulate lease expiration boundary crossed
            None
        }
    }

    fn reacquire_ownership(
        &self,
        stream: &kamish_meta::StreamId,
        region: &str,
        duration_ms: i64,
    ) -> Result<kamish_meta::PartitionOwnership, kamish_meta::MetaError> {
        self.inner.reacquire_ownership(stream, region, duration_ms)
    }
}

#[test]
fn test_append_crossing_lease_expiry_boundary_returns_unknown_outcome() {
    // Constraint 6: When a durable append commits to disk but the lease expires before
    // completion, produce MUST NOT return Ok; it must fail with lease_expired_outcome_unknown.
    let base_meta = Arc::new(MetadataStore::new_multi_region());
    let stream_id = kamish_meta::StreamId::new("expiry-topic", 0);
    base_meta
        .compare_and_swap_epoch(&stream_id, 0, 1, "region-a", 60_000)
        .unwrap();

    let meta = Arc::new(ExpiringDuringAppendMeta {
        inner: base_meta,
        call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });

    let (broker_base, _m, dir) = setup_multiregion_broker("region-a", vec!["region-b".into()]);
    let broker = BrokerServer::new(
        "region-a",
        broker_base.enforcer.clone(),
        meta,
        vec!["region-b".into()],
        dir.path(),
    );

    let produce_req = ProduceRequest {
        topic: "expiry-topic".to_string(),
        partition: 0,
        producer_id: 101,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "general".to_string(),
        payload: b"cross-expiry-data".to_vec(),
        residency_rule: None,
    };

    let result = broker.produce(produce_req);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, BrokerError::PolicyDenied(ref reason) if reason == "lease_expired_outcome_unknown"),
        "Expected lease_expired_outcome_unknown, got: {:?}",
        err
    );
}

#[test]
fn test_broker_restart_requires_explicit_reacquisition() {
    // Constraint 2: A broker must not restore local write authority from a persisted
    // ownership record without a valid reacquisition protocol.
    let (broker, meta, dir) = setup_multiregion_broker("region-a", vec!["region-b".into()]);
    let stream_id = kamish_meta::StreamId::new("restart-topic", 0);
    meta.compare_and_swap_epoch(&stream_id, 0, 1, "region-a", 60_000)
        .unwrap();

    let produce_req = ProduceRequest {
        topic: "restart-topic".to_string(),
        partition: 0,
        producer_id: 202,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "general".to_string(),
        payload: b"first-record".to_vec(),
        residency_rule: None,
    };

    // First produce succeeds
    let resp1 = broker
        .produce(produce_req.clone())
        .expect("First produce should succeed");
    assert_eq!(resp1.committed_epoch, 1);

    // Simulate broker crash and restart: lease was revoked/expired on shutdown.
    meta.revoke_lease(&stream_id, 0).unwrap();
    let enforcer = broker.enforcer.clone();
    drop(broker); // Release physical writer lock on shutdown (W14)

    let restarted_broker = BrokerServer::new(
        "region-a",
        enforcer,
        meta.clone(),
        vec!["region-b".into()],
        dir.path(),
    );

    // Write authority is NOT restored without reacquisition
    let req2 = ProduceRequest {
        sequence_base: 1,
        ..produce_req.clone()
    };
    let unauth_err = restarted_broker.produce(req2.clone()).unwrap_err();
    assert!(
        matches!(unauth_err, BrokerError::PolicyDenied(..)),
        "Restarted broker without reacquisition must deny produce, got: {:?}",
        unauth_err
    );

    // Now reacquire ownership explicitly
    restarted_broker
        .reacquire_partition_ownership("restart-topic", 0, 60_000)
        .expect("Explicit reacquisition must succeed");

    // Write authority is now valid; second produce succeeds
    let resp2 = restarted_broker
        .produce(req2)
        .expect("Produce after reacquisition must succeed");
    assert_eq!(resp2.committed_epoch, 1);
    assert_eq!(resp2.base_offset, 1);
}
