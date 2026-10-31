//! Research probes assert the intended contract and expose counterexamples.
//! See ../../PROBE_RESULTS.md for the reviewed revision and expected failures.
#[cfg(test)]
mod review {
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use kamish_audit::AuditLog;
    use kamish_broker::{BrokerServer, FetchRequest, ProduceRequest, ReplicationRequest};
    use kamish_enforce::Enforcer;
    use kamish_log::PartitionLog;
    use kamish_meta::MetadataStore;
    use kamish_policy::{
        AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
    };
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, Barrier, RwLock};

    fn signed_policy(secret_mode_at_a: DestinationMode) -> SignedPolicy {
        let key = SigningKey::from_bytes(&[17; 32]);
        let mut classes = HashMap::new();
        for i in 0..32 {
            let id = format!("class-{i}");
            classes.insert(
                id.clone(),
                ClassRule {
                    class_id: id,
                    description: String::new(),
                    legal_regions: ["a".to_string(), "b".to_string()].into(),
                    destination_modes: [
                        (
                            "a".to_string(),
                            if i == 1 {
                                secret_mode_at_a
                            } else {
                                DestinationMode::Cleartext
                            },
                        ),
                        (
                            "b".to_string(),
                            if i == 1 {
                                DestinationMode::SealedOnly
                            } else {
                                DestinationMode::Cleartext
                            },
                        ),
                    ]
                    .into(),
                    ack_mode: AckMode::LocalMajority,
                    failover_candidates: vec!["a".into(), "b".into()],
                    transit_allowed: false,
                },
            );
        }
        SignedPolicy::sign(
            PolicyDocument {
                policy_id: "review".into(),
                version: 1,
                issued_at_sec: Utc::now().timestamp() - 1,
                ttl_seconds: 3600,
                issuer_pubkey_hex: hex::encode(key.verifying_key().to_bytes()),
                classes,
                transit_rules: HashMap::new(),
            },
            &key,
        )
    }

    fn broker(region: &str, path: &Path, secret_mode_at_a: DestinationMode) -> BrokerServer {
        let signed = signed_policy(secret_mode_at_a);
        let mut cache = PolicyCache::new([signed.policy.issuer_pubkey_hex.clone()].into());
        cache.update(signed, Utc::now().timestamp()).unwrap();
        let enforcer = Arc::new(Enforcer::new(
            region,
            Arc::new(RwLock::new(cache)),
            AuditLog::new_in_memory(),
        ));
        BrokerServer::new(
            region,
            enforcer,
            Arc::new(MetadataStore::new()),
            vec!["b".into()],
            path,
        )
    }

    fn request(partition: u32, class_id: &str, payload: Vec<u8>) -> ProduceRequest {
        ProduceRequest {
            topic: "events".into(),
            partition,
            producer_id: 42,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: class_id.into(),
            payload,
            residency_rule: None,
        }
    }

    fn fetch(class_id: &str) -> FetchRequest {
        FetchRequest {
            topic: "events".into(),
            partition: 0,
            class_id: class_id.into(),
            start_offset: 0,
            max_records: 100,
        }
    }

    #[test]
    fn signed_policy_survives_wire_roundtrip() {
        let signed = signed_policy(DestinationMode::Cleartext);
        signed.verify_signature().unwrap();
        let decoded: SignedPolicy =
            serde_json::from_slice(&serde_json::to_vec(&signed).unwrap()).unwrap();
        assert_eq!(signed.policy, decoded.policy);
        assert!(
            decoded.verify_signature().is_ok(),
            "unchanged policy lost its valid signature after JSON roundtrip"
        );
    }

    #[test]
    fn ack_of_one_partition_preserves_other_partition() {
        let dir = tempfile::tempdir().unwrap();
        let source = broker("a", dir.path(), DestinationMode::Cleartext);
        source
            .produce(request(0, "class-0", b"partition-zero".to_vec()))
            .unwrap();
        source
            .produce(request(1, "class-0", b"partition-one".to_vec()))
            .unwrap();
        let outbox = source.get_outbox("b").unwrap();
        assert_eq!(outbox.next_transfer_batch(10).len(), 2);
        // Receiver has only acknowledged partition 0, offset 0.
        outbox.advance_partition_watermark("events", 0, 1);
        assert_eq!(
            outbox.next_transfer_batch(10).len(),
            1,
            "ack discarded the unreplicated partition-1 range"
        );
    }

    #[test]
    fn sealed_destination_does_not_persist_plaintext() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let source = broker("a", a.path(), DestinationMode::Cleartext);
        let target = broker("b", b.path(), DestinationMode::Cleartext);
        let secret = b"CANARY-sensitive-record".to_vec();
        let result = source.produce(request(0, "class-1", secret.clone()));
        if let Err(error) = result {
            // Explicit safe containment is acceptable, but must not be reported
            // as working sealed replication. Assert rejection before persistence.
            assert!(
                error
                    .to_string()
                    .contains("sealed_replication_not_configured")
            );
            assert!(
                source
                    .get_outbox("b")
                    .unwrap()
                    .next_transfer_batch(1)
                    .is_empty()
            );
            assert!(source.fetch(fetch("class-1")).unwrap().records.is_empty());
            return;
        }
        let range = source
            .get_outbox("b")
            .unwrap()
            .next_transfer_batch(1)
            .remove(0);
        assert!(range.is_sealed);
        // Same forwarding of payload/is_sealed used by ReplicationAgent::execute_task.
        let response = target
            .handle_replication(ReplicationRequest {
                topic: range.topic,
                partition: range.partition,
                source_region: "a".into(),
                target_region: "b".into(),
                class_id: range.class_id,
                epoch: range.epoch,
                from_offset: range.start_offset,
                to_offset: range.end_offset,
                payload: range.payload,
                is_sealed: range.is_sealed,
            })
            .unwrap();
        assert!(response.accepted);
        let persisted = PartitionLog::open(b.path().join("events-0"), 1)
            .unwrap()
            .read(0, 1);
        assert_ne!(
            persisted[0].payload, secret,
            "sealed-only receiver persisted the original plaintext"
        );
    }

    #[test]
    fn pending_replication_survives_broker_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let source = broker("a", dir.path(), DestinationMode::Cleartext);
            source
                .produce(request(0, "class-0", b"pending".to_vec()))
                .unwrap();
            assert_eq!(
                source
                    .get_outbox("b")
                    .unwrap()
                    .next_transfer_batch(10)
                    .len(),
                1
            );
        }
        let reopened = broker("a", dir.path(), DestinationMode::Cleartext);
        assert_eq!(reopened.fetch(fetch("class-0")).unwrap().records.len(), 1);
        assert_eq!(
            reopened
                .get_outbox("b")
                .unwrap()
                .next_transfer_batch(10)
                .len(),
            1,
            "durable log recovered but its pending replication vanished"
        );
    }

    #[test]
    fn oversized_outbox_write_is_not_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let source = broker("a", dir.path(), DestinationMode::Cleartext);
        let response = source.produce(request(0, "class-0", vec![0; 5_000_001]));
        assert!(
            !matches!(response, Ok(r) if r.sovereignty_allowed),
            "producer received success despite rejected replication enqueue"
        );
    }

    #[test]
    fn retry_after_restart_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let req = request(0, "class-0", b"retry".to_vec());
        {
            let source = broker("a", dir.path(), DestinationMode::Cleartext);
            assert_eq!(source.produce(req.clone()).unwrap().base_offset, 0);
        }
        let reopened = broker("a", dir.path(), DestinationMode::Cleartext);
        assert_eq!(
            reopened.produce(req).unwrap().base_offset,
            0,
            "retry created a new record after broker restart"
        );
    }

    #[test]
    fn concurrent_retries_append_once() {
        let dir = tempfile::tempdir().unwrap();
        let source = Arc::new(broker("a", dir.path(), DestinationMode::Cleartext));
        // Pre-open partition to isolate session atomicity from lazy log initialization.
        source.fetch(fetch("class-0")).unwrap();
        let gate = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let source = source.clone();
                let gate = gate.clone();
                std::thread::spawn(move || {
                    let req = request(0, "class-0", vec![1; 100_000]);
                    gate.wait();
                    source.produce(req).unwrap()
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(
            source.fetch(fetch("class-0")).unwrap().records.len(),
            1,
            "simultaneous retries bypassed session deduplication"
        );
    }

    #[test]
    fn fetch_cannot_bypass_record_policy_with_another_class() {
        let dir = tempfile::tempdir().unwrap();
        {
            let source = broker("a", dir.path(), DestinationMode::Cleartext);
            // This probe tests local fetch policy, not sealed replication.
            let mut req = request(0, "class-1", b"restricted".to_vec());
            req.residency_rule = Some(kamish_meta::ResidencyPolicy::StrictLocal);
            source.produce(req).unwrap();
        }
        // Reload a policy denying cleartext fetch of the existing record's class.
        let restricted = broker("a", dir.path(), DestinationMode::SealedOnly);
        assert!(restricted.fetch(fetch("class-1")).is_err());
        let result = restricted.fetch(fetch("class-0"));
        assert!(
            result.is_err() || result.unwrap().records.is_empty(),
            "request-supplied public class bypassed the stored record's fetch denial"
        );
    }

    #[test]
    fn recovery_preserves_per_record_residency() {
        use kamish_meta::{KamishPolicyEngine, ResidencyPolicy};
        let dir = tempfile::tempdir().unwrap();
        let source = broker("a", dir.path(), DestinationMode::Cleartext)
            .with_policy_engine(Arc::new(KamishPolicyEngine::new(AuditLog::new_in_memory())));
        let mut req = request(0, "class-0", b"local-only".to_vec());
        req.residency_rule = Some(ResidencyPolicy::StrictLocal);
        source.produce(req).unwrap();
        assert!(
            source
                .get_outbox("b")
                .unwrap()
                .next_transfer_batch(10)
                .is_empty()
        );
        source.repopulate_outbox_from_log("events", 0, "b").unwrap();
        assert!(
            source
                .get_outbox("b")
                .unwrap()
                .next_transfer_batch(10)
                .is_empty(),
            "outbox recovery discarded the original StrictLocal restriction"
        );
    }

    #[test]
    fn broker_obeys_metadata_owner() {
        let dir = tempfile::tempdir().unwrap();
        let source = broker("a", dir.path(), DestinationMode::Cleartext);
        source
            .meta
            .set_ownership("events", 0, "b", 7, 60_000)
            .unwrap();
        let result = source.produce(request(0, "class-0", b"wrong-owner".to_vec()));
        assert!(
            !matches!(result, Ok(r) if r.sovereignty_allowed),
            "broker a acknowledged epoch-1 write while metadata assigns ownership to b at epoch 7"
        );
    }

    #[test]
    fn audit_hash_binds_field_boundaries() {
        use kamish_audit::{AuditAction, AuditEntry, DecisionOutcome};
        let hash = |class, source| {
            AuditEntry::compute_hash(
                0,
                1,
                "genesis",
                AuditAction::BrokerAppend,
                class,
                source,
                "b",
                1,
                DecisionOutcome::AllowCleartext,
                "ok",
            )
        };
        assert_ne!(
            hash("ab", "c"),
            hash("a", "bc"),
            "different audit field values produce identical serialized hash input"
        );
    }
}
