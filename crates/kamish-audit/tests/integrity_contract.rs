use kamish_audit::{AuditAction, AuditEntry, AuditError, AuditLog, DecisionOutcome};

fn record(log: &AuditLog) -> Result<AuditEntry, AuditError> {
    log.record(
        AuditAction::BrokerAppend,
        "class",
        "eu",
        "us",
        1,
        DecisionOutcome::AllowCleartext,
        "ok",
    )
}

#[test]
fn adjacent_field_boundaries_are_authenticated() {
    let hash = |class, source| {
        AuditEntry::compute_hash(
            0,
            1,
            "genesis",
            AuditAction::BrokerAppend,
            class,
            source,
            "us",
            1,
            DecisionOutcome::AllowCleartext,
            "ok",
        )
    };
    assert_ne!(hash("ab", "c"), hash("a", "bc"));
    assert_ne!(hash("", "a"), hash("a", ""));
}

#[test]
fn disk_tampering_by_repartitioning_fields_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.log");
    let log = AuditLog::open(&path).unwrap();
    let mut entry = log
        .record(
            AuditAction::BrokerAppend,
            "ab",
            "c",
            "us",
            1,
            DecisionOutcome::AllowCleartext,
            "ok",
        )
        .unwrap();
    entry.class_id = "a".into();
    entry.source_region = "bc".into();
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&entry).unwrap()),
    )
    .unwrap();
    assert!(matches!(
        AuditLog::open(path),
        Err(AuditError::EntryHashMismatch { .. })
    ));
}

#[test]
fn unsupported_format_is_rejected_without_modifying_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.log");
    let mut entry = record(&AuditLog::new_in_memory()).unwrap();
    entry.format_version = 0;
    let bytes = serde_json::to_vec(&entry).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        AuditLog::open(&path),
        Err(AuditError::UnsupportedFormat(0))
    ));
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn failed_disk_append_latches_writer_until_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.log");
    let saved = dir.path().join("saved.log");
    let log = AuditLog::open(&path).unwrap();
    record(&log).unwrap();
    // Replace only this test's sink with a directory: deterministic I/O failure,
    // including when tests run as root (unlike chmod-based fault injection).
    std::fs::rename(&path, &saved).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(record(&log).is_err());
    assert_eq!(log.count(), 1);
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(saved, &path).unwrap();
    assert!(matches!(record(&log), Err(AuditError::WriterFailed)));
    let reopened = AuditLog::open(path).unwrap();
    assert_eq!(record(&reopened).unwrap().sequence, 1);
}

#[test]
fn read_only_verification_never_creates_missing_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.log");
    assert!(AuditLog::verify_file(&missing).is_err());
    assert!(!missing.exists());
    let existing = dir.path().join("existing.log");
    let log = AuditLog::open(&existing).unwrap();
    record(&log).unwrap();
    let original = std::fs::read(&existing).unwrap();
    assert_eq!(AuditLog::verify_file(&existing).unwrap(), 1);
    assert_eq!(std::fs::read(&existing).unwrap(), original);
}
