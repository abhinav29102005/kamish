//! Additional application-test counterexamples. These assert the intended
//! stronger contracts, not the limitations of the current prototype.
//! They are intentionally outside cargo test --workspace; see the dated report.
use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::{AuditAction, AuditLog, DecisionOutcome};
use kamish_broker::{BrokerServer, ProduceRequest, ReplicationRequest};
use kamish_enforce::Enforcer;
use kamish_meta::{MetadataStore, ResidencyPolicy};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use std::sync::{Arc, RwLock};

fn broker_with_authority(
    region: &str,
    peers: Vec<String>,
    meta: Arc<dyn kamish_meta::MetadataAuthority>,
) -> (BrokerServer, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let key = SigningKey::from_bytes(&[31; 32]);
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let rule = ClassRule {
        class_id: "general".into(),
        description: "synthetic gap probes".into(),
        legal_regions: ["a".into(), "b".into()].into(),
        destination_modes: [
            ("a".into(), DestinationMode::Cleartext),
            ("b".into(), DestinationMode::Cleartext),
        ]
        .into(),
        ack_mode: AckMode::LocalMajority,
        failover_candidates: vec!["a".into(), "b".into()],
        transit_allowed: false,
    };
    let doc = PolicyDocument {
        policy_id: "gap-tests".into(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 1,
        ttl_seconds: 3600,
        issuer_pubkey_hex: public_key.clone(),
        classes: [("general".into(), rule)].into(),
        transit_rules: Default::default(),
    };
    let mut cache = PolicyCache::new([public_key].into());
    cache
        .update(SignedPolicy::sign(doc, &key), Utc::now().timestamp())
        .unwrap();
    let enforcer = Arc::new(Enforcer::new(
        region,
        Arc::new(RwLock::new(cache)),
        AuditLog::new_in_memory(),
    ));
    (
        BrokerServer::try_new(region, enforcer, meta, peers, dir.path()).unwrap(),
        dir,
    )
}

fn broker(region: &str, peers: Vec<String>) -> (BrokerServer, tempfile::TempDir) {
    broker_with_authority(region, peers, Arc::new(MetadataStore::new()))
}

fn transfer(epoch: u64, to_offset: u64, payload: &[u8]) -> ReplicationRequest {
    ReplicationRequest {
        topic: "events".into(),
        partition: 0,
        source_region: "a".into(),
        target_region: "b".into(),
        class_id: "general".into(),
        epoch,
        from_offset: 0,
        to_offset,
        payload: payload.to_vec(),
        is_sealed: false,
        context: None,
        source_generation: 1,
        policy_version: 1,
        checksum: None,
        envelope_payload: None,
    }
}

fn produce(sequence: u32, restriction: ResidencyPolicy) -> ProduceRequest {
    ProduceRequest {
        topic: "events".into(),
        partition: 0,
        producer_id: 42,
        producer_epoch: 1,
        sequence_base: sequence,
        class_id: "general".into(),
        payload: format!("event-{sequence}").into_bytes(),
        residency_rule: Some(restriction),
    }
}

#[test]
fn stale_epoch_duplicate_must_not_acknowledge_a_new_epoch_history() {
    let (receiver, _dir) = broker("b", vec![]);
    assert!(
        receiver
            .handle_replication(transfer(7, 1, b"new-epoch"))
            .unwrap()
            .accepted
    );
    let stale = receiver
        .handle_replication(transfer(1, 1, b"old-epoch"))
        .unwrap();
    assert!(
        !stale.accepted,
        "stale epoch 1 received a successful duplicate ACK for epoch 7 data"
    );
}

#[test]
fn duplicate_offset_with_different_content_must_be_rejected() {
    let (receiver, _dir) = broker("b", vec![]);
    assert!(
        receiver
            .handle_replication(transfer(1, 1, b"first"))
            .unwrap()
            .accepted
    );
    let conflict = receiver
        .handle_replication(transfer(1, 1, b"different"))
        .unwrap();
    assert!(
        !conflict.accepted,
        "same offset with different bytes was acknowledged as identical"
    );
}

#[test]
fn claimed_multi_record_range_must_match_the_persisted_record_count() {
    let (receiver, _dir) = broker("b", vec![]);
    let result = receiver
        .handle_replication(transfer(1, 5, b"one opaque record"))
        .unwrap();
    assert!(
        !result.accepted || result.applied_watermark == 5,
        "range [0,5) was accepted but only one record was durably applied"
    );
}

#[test]
fn independent_regions_must_not_both_accept_unregistered_epoch_one_writes() {
    let authority: Arc<dyn kamish_meta::MetadataAuthority> = Arc::new(
        MetadataStore::new_with_mode(kamish_meta::DeploymentMode::MultiRegion),
    );
    let (a, _a) = broker_with_authority("a", vec![], authority.clone());
    let (b, _b) = broker_with_authority("b", vec![], authority);
    let a_result = a.produce(produce(0, ResidencyPolicy::GlobalCleartext));
    let b_result = b.produce(produce(0, ResidencyPolicy::GlobalCleartext));
    let both_acked = matches!(a_result, Ok(ref r) if r.sovereignty_allowed && r.committed_epoch == 1)
        && matches!(b_result, Ok(ref r) if r.sovereignty_allowed && r.committed_epoch == 1);
    assert!(
        !both_acked,
        "independent regions both acknowledged partition events/0 at epoch 1"
    );
}

#[test]
fn mixed_replication_contract_must_not_create_an_unfillable_mirror_gap() {
    let (source, _a) = broker("a", vec!["b".into()]);
    let (receiver, _b) = broker("b", vec![]);
    assert!(
        source
            .produce(produce(0, ResidencyPolicy::StrictLocal))
            .unwrap()
            .sovereignty_allowed
    );
    let second = source
        .produce(produce(1, ResidencyPolicy::GlobalCleartext))
        .unwrap();
    if !second.sovereignty_allowed {
        return;
    }
    let range = source
        .get_outbox("b")
        .unwrap()
        .next_transfer_batch(1)
        .remove(0);
    let mut req = transfer(range.epoch, range.end_offset, &range.payload);
    req.from_offset = range.start_offset;
    let response = receiver.handle_replication(req).unwrap();
    assert!(
        response.accepted,
        "source accepted a replication-contract change that leaves destination missing forbidden offset 0"
    );
}

#[test]
fn audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.log");

    // 1. Negative control: without external witness checkpoint, tail removal is undetectable
    {
        let neg_path = dir.path().join("neg_audit.log");
        let audit = AuditLog::open(&neg_path).unwrap();
        audit
            .record(
                AuditAction::Ingress,
                "general",
                "a",
                "a",
                1,
                DecisionOutcome::AllowCleartext,
                "first",
            )
            .unwrap();
        audit
            .record(
                AuditAction::Ingress,
                "general",
                "a",
                "a",
                1,
                DecisionOutcome::AllowCleartext,
                "second",
            )
            .unwrap();
        drop(audit);
        let neg_text = std::fs::read_to_string(&neg_path).unwrap();
        std::fs::write(
            &neg_path,
            format!(
                "{}
",
                neg_text.lines().next().unwrap()
            ),
        )
        .unwrap();
        assert!(
            AuditLog::open(&neg_path).is_ok(),
            "negative control: tail removal is undetectable without witness"
        );
    }

    // 2. Main probe fixture: attach external consensus-backed witness checkpoint
    let meta = Arc::new(MetadataStore::new());
    let signing_key = SigningKey::from_bytes(&[42; 32]);
    let witness = Arc::new(kamish_audit::AuditWitness::new(
        meta.clone(),
        "general",
        0,
        signing_key,
        128,
    ));
    AuditLog::register_witness(&path, witness.clone());

    let last_hash = {
        let audit = AuditLog::open(&path).unwrap();
        for reason in ["first", "second"] {
            audit
                .record(
                    AuditAction::Ingress,
                    "general",
                    "a",
                    "a",
                    1,
                    DecisionOutcome::AllowCleartext,
                    reason,
                )
                .unwrap();
        }
        audit.last_hash()
    };

    // External witness commits trusted checkpoint covering both records
    witness.checkpoint(&last_hash, 2).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        format!(
            "{}
",
            text.lines().next().unwrap()
        ),
    )
    .unwrap();

    // Assertion unchanged: removing valid audit suffix is detected via witness checkpoint!
    assert!(
        AuditLog::open(&path).is_err(),
        "removing a valid audit suffix was undetectable without an external checkpoint"
    );

    AuditLog::unregister_witness(&path);
}
