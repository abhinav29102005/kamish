use kamish_log::{PartitionLog, RecordContext, SessionManager};

fn context() -> RecordContext {
    RecordContext {
        producer_id: 42, producer_epoch: 3, sequence: 0,
        origin_region: "eu".into(), request_policy: "StrictLocal".into(),
        replication_targets: vec![],
    }
}

#[test]
fn context_survives_restart_and_is_covered_by_checksum() {
    let dir = tempfile::tempdir().unwrap();
    {
        let log = PartitionLog::open(dir.path(), 1).unwrap();
        log.append_with_context(7, "class", 2, b"payload".to_vec(), Some(context())).unwrap();
    }
    let log = PartitionLog::open(dir.path(), 1).unwrap();
    let mut record = log.read(0, 1).remove(0);
    assert_eq!(record.context.as_ref(), Some(&context()));
    assert!(record.verify_checksum());
    record.context.as_mut().unwrap().replication_targets.push("us".into());
    assert!(!record.verify_checksum());
    assert!(log.append(1, "class", 2, b"stale".to_vec()).is_err());
}

#[test]
fn legacy_records_remain_readable_without_fabricated_admission_context() {
    let dir = tempfile::tempdir().unwrap();
    PartitionLog::open(dir.path(), 1).unwrap().append(1, "class", 1, vec![1]).unwrap();
    let record = PartitionLog::open(dir.path(), 1).unwrap().read(0, 1).remove(0);
    assert!(record.context.is_none());
    assert!(record.verify_checksum());
}

#[test]
fn new_producer_epoch_resets_sequence_and_stale_epoch_is_rejected() {
    let sessions = SessionManager::new();
    sessions.update(42, 1, 10, 10);
    assert_eq!(sessions.check_sequence(42, 2, 0).unwrap(), None);
    assert!(sessions.check_sequence(42, 2, 11).is_err());
    sessions.update(42, 2, 0, 11);
    assert!(sessions.check_sequence(42, 1, 0).is_err());
    assert_eq!(sessions.check_sequence(42, 2, 0).unwrap(), Some(11));
}
