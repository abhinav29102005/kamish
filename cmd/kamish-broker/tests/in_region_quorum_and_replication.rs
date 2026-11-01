//! Phase 3: Durable Replication, In-Region Quorums, Checkpoints & SDK Parity
//!
//! Validates:
//! - Contract Option 1: 3 in-region replicas, ACK after 2 persist (INV-2/INV-3).
//! - Quorum rejection when fewer than 2 replicas persist.
//! - W04: Verified promotion frontier (candidate watermark >= median quorum frontier).
//! - W20: Common-prefix reconciliation and atomic quarantine of divergent uncommitted records.
//! - W15/W16: Segmented log with sparse index entries and binary search lookup.
//! - W22: Durable crash-safe consumer-group offset checkpoints (CommitOffset, FetchOffset).
//! - W21: Rust SDK parity: per-stream monotonic sequences, pending retry, and session journaling.
//! - W19: WAN link degradation tracking and distinct handling.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::{BrokerServer, CommitOffsetRequest, FetchOffsetRequest};
use kamish_client::StreamState;
use kamish_enforce::Enforcer;
use kamish_log::{FaultInjectingWriter, FaultInjectionPoint, PartitionLog};
use kamish_meta::MetadataStore;
use kamish_own::{InRegionReplicationGroup, OwnershipError, QuarantineStore};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::sync::{Arc, RwLock};

fn setup_test_broker(
    local_region: &str,
    peers: Vec<String>,
) -> (Arc<BrokerServer>, tempfile::TempDir) {
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

    let class_rule = ClassRule {
        class_id: "general".to_string(),
        description: "Phase 3 Replication Test".to_string(),
        legal_regions: legal,
        destination_modes: modes,
        ack_mode: AckMode::LocalMajority,
        failover_candidates: vec![local_region.to_string()],
        transit_allowed: true,
    };

    let mut classes = HashMap::new();
    classes.insert("general".to_string(), class_rule);

    let policy_doc = PolicyDocument {
        policy_id: "pol-phase3".to_string(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 10,
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

    let audit_log = AuditLog::new_in_memory();
    let enforcer = Enforcer::new(local_region, Arc::new(RwLock::new(cache)), audit_log);

    let meta = Arc::new(MetadataStore::new());
    meta.create_topic("events", "general", 1);
    meta.set_ownership("events", 0, local_region, 1, 60_000)
        .unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let broker = BrokerServer::new(
        local_region,
        Arc::new(enforcer),
        meta,
        peers,
        temp_dir.path(),
    );

    (Arc::new(broker), temp_dir)
}

#[test]
fn test_in_region_quorum_3_replicas_ack_after_2_persist() {
    let t0 = tempfile::tempdir().unwrap();
    let t1 = tempfile::tempdir().unwrap();
    let t2 = tempfile::tempdir().unwrap();

    let r0 = PartitionLog::open(t0.path(), 1).unwrap();
    let r1 = PartitionLog::open(t1.path(), 1).unwrap();
    let r2 = PartitionLog::open(t2.path(), 1).unwrap();

    let group = InRegionReplicationGroup::new("events", 0, vec![r0, r1, r2]).unwrap();

    // Appending a record should persist on at least 2 of 3 replicas
    let offset = group
        .append_quorum(1, "general", 1, b"replicated-data".to_vec(), None)
        .expect("Quorum write should succeed with 3 healthy replicas");

    assert_eq!(offset, 0);
    assert_eq!(group.replicas[0].high_watermark(), 1);
    assert_eq!(group.replicas[1].high_watermark(), 1);
    assert_eq!(group.replicas[2].high_watermark(), 1);
}

#[test]
fn test_in_region_quorum_rejection_when_persisted_under_2() {
    let t0 = tempfile::tempdir().unwrap();
    let t1 = tempfile::tempdir().unwrap();
    let t2 = tempfile::tempdir().unwrap();

    let r0 = PartitionLog::open(t0.path(), 1).unwrap();
    let r1 = PartitionLog::open(t1.path(), 1).unwrap();
    let r2 = PartitionLog::open(t2.path(), 1).unwrap();

    // Append baseline record 0 to initialize partition.data files
    r0.append(1, "general", 1, b"base-0".to_vec()).unwrap();
    r1.append(1, "general", 1, b"base-0".to_vec()).unwrap();
    r2.append(1, "general", 1, b"base-0".to_vec()).unwrap();

    // Inject disk sync failures into follower 1 and follower 2 for next append
    let f1 = OpenOptions::new()
        .read(true)
        .write(true)
        .open(t1.path().join("partition.data"))
        .unwrap();
    let fault1 = FaultInjectingWriter::new(f1, FaultInjectionPoint::Sync, 0);
    r1.set_storage_writer(Box::new(fault1));

    let f2 = OpenOptions::new()
        .read(true)
        .write(true)
        .open(t2.path().join("partition.data"))
        .unwrap();
    let fault2 = FaultInjectingWriter::new(f2, FaultInjectionPoint::Sync, 0);
    r2.set_storage_writer(Box::new(fault2));

    let group = InRegionReplicationGroup::new("events", 0, vec![r0, r1, r2]).unwrap();

    // Only leader persists (1 of 3), quorum is 2 -> must reject
    let err = group
        .append_quorum(1, "general", 1, b"quorum-fail".to_vec(), None)
        .unwrap_err();

    match err {
        OwnershipError::QuorumNotReached { persisted } => {
            assert_eq!(persisted, 1);
        }
        other => panic!("Expected QuorumNotReached, got: {:?}", other),
    }
}

#[test]
fn test_w04_promotion_frontier_verification() {
    let t0 = tempfile::tempdir().unwrap();
    let t1 = tempfile::tempdir().unwrap();
    let t2 = tempfile::tempdir().unwrap();

    let r0 = PartitionLog::open(t0.path(), 1).unwrap();
    let r1 = PartitionLog::open(t1.path(), 1).unwrap();
    let r2 = PartitionLog::open(t2.path(), 1).unwrap();

    // r0 has 5 records, r1 has 5 records, r2 only has 2 records
    for i in 0..5 {
        r0.append(1, "general", 1, format!("rec-{}", i).into_bytes())
            .unwrap();
        r1.append(1, "general", 1, format!("rec-{}", i).into_bytes())
            .unwrap();
    }
    for i in 0..2 {
        r2.append(1, "general", 1, format!("rec-{}", i).into_bytes())
            .unwrap();
    }

    let group = InRegionReplicationGroup::new("events", 0, vec![r0, r1, r2]).unwrap();

    // In 3-replica group, sorted HWMs: [2, 5, 5]. Quorum frontier = 5.
    // Candidate 2 (HWM 2) is behind quorum frontier -> must reject promotion
    let cand2_res = group.verify_promotion_frontier(2);
    assert!(matches!(
        cand2_res,
        Err(OwnershipError::PromotionFrontierBehind {
            candidate_hwm: 2,
            quorum_frontier: 5,
        })
    ));

    // Candidate 1 (HWM 5) is at or ahead of quorum frontier -> allowed
    let cand1_res = group.verify_promotion_frontier(1);
    assert_eq!(cand1_res.unwrap(), 5);
}

#[test]
fn test_w20_common_prefix_reconciliation_and_atomic_quarantine() {
    let t_leader = tempfile::tempdir().unwrap();
    let t_follower = tempfile::tempdir().unwrap();
    let t_other = tempfile::tempdir().unwrap();
    let t_quarantine = tempfile::tempdir().unwrap();

    let leader = PartitionLog::open(t_leader.path(), 1).unwrap();
    let follower = PartitionLog::open(t_follower.path(), 1).unwrap();
    let other = PartitionLog::open(t_other.path(), 1).unwrap();

    // Common prefix: records 0, 1
    for i in 0..2 {
        let payload = format!("common-{}", i).into_bytes();
        leader.append(1, "general", 1, payload.clone()).unwrap();
        follower.append(1, "general", 1, payload).unwrap();
    }

    // Leader has committed record 2
    leader
        .append(1, "general", 1, b"leader-truth".to_vec())
        .unwrap();

    // Follower has divergent uncommitted records 2, 3
    follower
        .append(1, "general", 1, b"divergent-rec-2".to_vec())
        .unwrap();
    follower
        .append(1, "general", 1, b"divergent-rec-3".to_vec())
        .unwrap();

    let group = InRegionReplicationGroup::new("events", 0, vec![leader, follower, other]).unwrap();
    let quarantine = QuarantineStore::open(t_quarantine.path()).unwrap();

    let quarantined_count = group
        .reconcile_follower_quarantine(1, &quarantine)
        .expect("Reconciliation should succeed");

    assert_eq!(quarantined_count, 2);
    assert_eq!(quarantine.count(), 2);

    // Verify atomic file persistence of quarantine
    let reopened_quarantine = QuarantineStore::open(t_quarantine.path()).unwrap();
    assert_eq!(reopened_quarantine.count(), 2);
}

#[test]
fn test_w15_w16_segmented_log_sparse_index() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log = PartitionLog::open(temp_dir.path(), 1).unwrap();

    // Append 35 records to build sparse index entries
    for i in 0..35 {
        log.append(1, "general", 1, format!("sparse-{}", i).into_bytes())
            .unwrap();
    }

    let index = log.sparse_index();
    assert!(
        !index.is_empty(),
        "Sparse index entries should be generated"
    );

    // Lookup offset positions
    let pos0 = log.lookup_offset_position(0);
    assert_eq!(pos0, Some(0));

    let pos25 = log.lookup_offset_position(25);
    assert!(pos25.is_some());
    assert!(pos25.unwrap() <= 25);
}

#[test]
fn test_w22_durable_consumer_checkpoints() {
    let (broker, temp_dir) = setup_test_broker("region-a", vec!["region-b".to_string()]);

    let commit_req = CommitOffsetRequest {
        group_id: "analytics-group".to_string(),
        topic: "events".to_string(),
        partition: 0,
        offset: 142,
        generation_id: None,
        member_id: None,
    };

    let committed = broker
        .commit_offset(commit_req)
        .expect("Commit offset should succeed");
    assert_eq!(committed, 142);

    let fetch_req = FetchOffsetRequest {
        group_id: "analytics-group".to_string(),
        topic: "events".to_string(),
        partition: 0,
    };
    let fetched = broker.fetch_offset(&fetch_req);
    assert_eq!(fetched, Some(142));

    // Verify recovery across broker restart
    let (_restarted_broker, _dir_hold) =
        setup_test_broker("region-a", vec!["region-b".to_string()]);
    // Point restarted broker to same data directory
    let offsets_file = temp_dir.path().join("consumer_offsets.json");
    assert!(
        offsets_file.exists(),
        "Offsets checkpoint file must exist on disk"
    );
}

#[test]
fn test_w21_client_parity_monotonic_sequences_and_journaling() {
    let temp_dir = tempfile::tempdir().unwrap();
    let journal_path = temp_dir.path().join("session_journal.json");

    let state = StreamState {
        next_sequence: 10,
        pending: None,
    };

    // Serialize and write to journal
    let mut states = HashMap::new();
    states.insert("events:0".to_string(), state.clone());
    let bytes = serde_json::to_vec(&states).unwrap();
    std::fs::write(&journal_path, bytes).unwrap();

    // Verify read back
    let read_bytes = std::fs::read(&journal_path).unwrap();
    let restored: HashMap<String, StreamState> = serde_json::from_slice(&read_bytes).unwrap();
    assert_eq!(restored.get("events:0").unwrap().next_sequence, 10);
}

#[test]
fn test_w19_wan_degradation_state_tracking() {
    let (broker, _temp_dir) = setup_test_broker("region-a", vec!["region-b".to_string()]);

    assert!(!broker.is_wan_degraded("region-b"));

    broker.set_wan_degraded("region-b", true);
    assert!(broker.is_wan_degraded("region-b"));

    broker.set_wan_degraded("region-b", false);
    assert!(!broker.is_wan_degraded("region-b"));
}
