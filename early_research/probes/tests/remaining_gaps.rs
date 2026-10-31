//! Additional application-test counterexamples. These assert the intended
//! stronger contracts, not the limitations of the current prototype.
//! They are intentionally outside cargo test --workspace; see the dated report.
use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::{AuditAction, AuditLog, AuditWitness, DecisionOutcome};
use kamish_broker::{BrokerServer, ProduceRequest, ReplicationRequest};
use kamish_enforce::Enforcer;
use kamish_meta::{MetadataStore, ResidencyPolicy};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use std::sync::{Arc, RwLock};

fn broker(region: &str, peers: Vec<String>) -> (BrokerServer, tempfile::TempDir) {
    broker_with_authority(region, peers, Arc::new(MetadataStore::new()))
}

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
    let meta: Arc<dyn kamish_meta::MetadataAuthority> = if std::env::var("KAMISH_ETCD_TESTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        Arc::new(
            kamish_meta::EtcdMetadataAuthority::new(
                vec![
                    "https://127.0.0.1:2379".to_string(),
                    "https://127.0.0.1:2381".to_string(),
                    "https://127.0.0.1:2383".to_string(),
                ],
                Some(root.join("certs/ca.crt")),
                Some(root.join("certs/client.crt")),
                Some(root.join("certs/client.key")),
                None,
            )
            .expect("Failed to connect to etcd"),
        )
    } else {
        Arc::new(MetadataStore::new_multi_region())
    };

    let (a, _a) = broker_with_authority("a", vec![], meta.clone());
    let (b, _b) = broker_with_authority("b", vec![], meta.clone());

    // 1. Both brokers race to claim partition ownership for events/0 at epoch 1
    let a_claim = a.claim_partition_ownership("events", 0, 0, 1, 30_000);
    let b_claim = b.claim_partition_ownership("events", 0, 0, 1, 30_000);

    // Exactly one claimant wins; the other honestly receives EpochFenced
    assert!(
        (a_claim.is_ok() && matches!(b_claim, Err(kamish_broker::BrokerError::EpochFenced { .. })))
            || (b_claim.is_ok()
                && matches!(a_claim, Err(kamish_broker::BrokerError::EpochFenced { .. }))),
        "Consensus must grant epoch 1 to exactly one claimant and fence the other. a: {:?}, b: {:?}",
        a_claim,
        b_claim
    );

    // 2. Both brokers attempt produce at epoch 1
    let a_result = a.produce(produce(0, ResidencyPolicy::GlobalCleartext));
    let b_result = b.produce(produce(0, ResidencyPolicy::GlobalCleartext));

    // 3. UNTOUCHED ORIGINAL ASSERTION: Both must not acknowledge epoch 1
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
    let meta: Arc<dyn kamish_meta::MetadataAuthority> = if std::env::var("KAMISH_ETCD_TESTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        Arc::new(
            kamish_meta::EtcdMetadataAuthority::new(
                vec![
                    "https://127.0.0.1:2379".to_string(),
                    "https://127.0.0.1:2381".to_string(),
                    "https://127.0.0.1:2383".to_string(),
                ],
                Some(root.join("certs/ca.crt")),
                Some(root.join("certs/client.crt")),
                Some(root.join("certs/client.key")),
                None,
            )
            .expect("Failed to connect to etcd"),
        )
    } else {
        Arc::new(kamish_meta::MetadataStore::new_multi_region())
    };

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.log");

    let key = SigningKey::from_bytes(&[42u8; 32]);
    let witness = Arc::new(AuditWitness::new(
        meta.clone(),
        "audit-stream-a",
        0,
        key.clone(),
        128,
    ));

    // Register witness for this path
    AuditLog::register_witness(&path, witness.clone());

    {
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
        // Explicitly anchor checkpoint for the two written entries
        witness.checkpoint(&audit.last_hash(), 2).unwrap();
    }

    // --- Control 1: Untouched-file positive control ---
    assert!(
        AuditLog::open(&path).is_ok(),
        "registered, checkpointed file must reopen successfully"
    );

    // --- Control 2: No-witness negative control ---
    // Demonstrate that without an external checkpoint, suffix removal is undetectable
    let unanchored_path = dir.path().join("unanchored.log");
    let text = std::fs::read_to_string(&path).unwrap();
    let truncated_text = format!(
        "{}
",
        text.lines().next().unwrap()
    );
    std::fs::write(&unanchored_path, &truncated_text).unwrap();
    assert!(
        AuditLog::verify_file(&unanchored_path).is_ok(),
        "without an external checkpoint, an internally-valid prefix is indistinguishable from the true log"
    );

    // --- Control 3: Tamper inside anchored prefix detected ---
    // (This is the original tamper operation: truncate line 2)
    std::fs::write(&path, truncated_text).unwrap();

    // UNTOUCHED ORIGINAL TAMPER ASSERTION:
    assert!(
        AuditLog::open(&path).is_err(),
        "removing a valid audit suffix was undetectable without an external checkpoint"
    );

    // --- Control 4: Unanchored-window limitation test ---
    // Anchor C entries, append within cap, remove portion solely after C, assert undetectable
    let window_path = dir.path().join("window.log");
    let window_witness = Arc::new(AuditWitness::new(
        meta.clone(),
        "audit-window-stream",
        0,
        key.clone(),
        128,
    ));
    AuditLog::register_witness(&window_path, window_witness.clone());
    {
        let audit = AuditLog::open(&window_path).unwrap();
        audit
            .record(
                AuditAction::Ingress,
                "general",
                "a",
                "a",
                1,
                DecisionOutcome::AllowCleartext,
                "entry_0",
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
                "entry_1",
            )
            .unwrap();
        window_witness.checkpoint(&audit.last_hash(), 2).unwrap();

        audit
            .record(
                AuditAction::Ingress,
                "general",
                "a",
                "a",
                1,
                DecisionOutcome::AllowCleartext,
                "entry_2_unanchored",
            )
            .unwrap();
    }
    // Remove the unanchored 3rd entry
    let w_text = std::fs::read_to_string(&window_path).unwrap();
    let w_lines: Vec<&str> = w_text.lines().collect();
    std::fs::write(
        &window_path,
        format!(
            "{}
{}
",
            w_lines[0], w_lines[1]
        ),
    )
    .unwrap();

    // The external anchor C=2 cannot detect deletion purely in the unanchored suffix:
    let report = window_witness.verify_file(&window_path).unwrap();
    assert_eq!(report.anchored_count, 2);
    assert_eq!(report.total_entries, 2);
    assert_eq!(
        report.unanchored_entries, 0,
        "tamper within unanchored window cannot be detected by external anchor C"
    );

    // Clean up witness registration
    AuditLog::unregister_witness(&path);
    AuditLog::unregister_witness(&window_path);
}
