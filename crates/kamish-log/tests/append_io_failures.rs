use kamish_log::{FaultInjectingWriter, FaultInjectionPoint, LogError, PartitionLog};
use std::fs::OpenOptions;

#[test]
fn test_length_write_failure_rollback_and_subsequent_append_success() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    // 1. Commit baseline record 0
    let o0 = log
        .append(1, "audit-class", 1, b"baseline-record-0".to_vec())
        .unwrap();
    assert_eq!(o0, 0);
    assert_eq!(log.next_offset(), 1);
    assert_eq!(log.high_watermark(), 1);

    let baseline_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();

    // 2. Inject length-write failure with 2 partial bytes
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::LengthWrite, 2);
    log.set_storage_writer(Box::new(fault_writer));

    // Append attempt must fail and safely roll back
    let err = log
        .append(1, "audit-class", 1, b"failing-record-1".to_vec())
        .unwrap_err();
    assert!(
        matches!(err, LogError::Io(_)),
        "Expected Io error, got {:?}",
        err
    );

    // Invariants preserved: writer not fenced, offset not advanced, HWM not advanced
    assert!(!log.is_fenced());
    assert_eq!(log.next_offset(), 1);
    assert_eq!(log.high_watermark(), 1);

    // On-disk file length must have been rolled back to baseline_len
    let current_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();
    assert_eq!(
        current_len, baseline_len,
        "File must be rolled back to pre-append length"
    );

    // 3. Subsequent append must succeed at original next_offset (1)
    let o1 = log
        .append(1, "audit-class", 1, b"recovered-record-1".to_vec())
        .unwrap();
    assert_eq!(o1, 1);
    assert_eq!(log.next_offset(), 2);
    assert_eq!(log.high_watermark(), 2);

    // 4. Reopen and verify data integrity (drop previous writer lock first)
    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert_eq!(reopened.next_offset(), 2);
    assert_eq!(reopened.high_watermark(), 2);
    let records = reopened.read(0, 10);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].base_offset, 0);
    assert_eq!(records[0].payload, b"baseline-record-0");
    assert_eq!(records[1].base_offset, 1);
    assert_eq!(records[1].payload, b"recovered-record-1");
    assert!(records[0].verify_checksum());
    assert!(records[1].verify_checksum());
}

#[test]
fn test_body_write_failure_rollback_and_subsequent_append_success() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    let o0 = log
        .append(1, "audit-class", 1, b"baseline-record-0".to_vec())
        .unwrap();
    assert_eq!(o0, 0);

    let baseline_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();

    // Inject body-write failure with 10 partial payload bytes
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::BodyWrite, 10);
    log.set_storage_writer(Box::new(fault_writer));

    let err = log
        .append(1, "audit-class", 1, b"failing-record-with-body".to_vec())
        .unwrap_err();
    assert!(matches!(err, LogError::Io(_)));

    assert!(!log.is_fenced());
    assert_eq!(log.next_offset(), 1);
    assert_eq!(log.high_watermark(), 1);

    let current_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();
    assert_eq!(current_len, baseline_len);

    // Subsequent append succeeds at offset 1
    let o1 = log
        .append(1, "audit-class", 1, b"clean-body-record-1".to_vec())
        .unwrap();
    assert_eq!(o1, 1);
    assert_eq!(log.next_offset(), 2);
    assert_eq!(log.high_watermark(), 2);

    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert_eq!(reopened.high_watermark(), 2);
    let records = reopened.read(0, 10);
    assert_eq!(records[1].payload, b"clean-body-record-1");
}

#[test]
fn test_flush_failure_rollback_and_subsequent_append_success() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    log.append(1, "audit-class", 1, b"record-0".to_vec())
        .unwrap();
    let baseline_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();

    // Inject flush failure
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::Flush, 0);
    log.set_storage_writer(Box::new(fault_writer));

    let err = log
        .append(1, "audit-class", 1, b"record-1-flush-fail".to_vec())
        .unwrap_err();
    assert!(matches!(err, LogError::Io(_)));

    assert!(!log.is_fenced());
    assert_eq!(log.next_offset(), 1);
    assert_eq!(log.high_watermark(), 1);

    let current_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();
    assert_eq!(current_len, baseline_len);

    let o1 = log
        .append(1, "audit-class", 1, b"record-1-success".to_vec())
        .unwrap();
    assert_eq!(o1, 1);
    assert_eq!(log.next_offset(), 2);
    assert_eq!(log.high_watermark(), 2);

    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert_eq!(reopened.high_watermark(), 2);
    assert_eq!(reopened.read(1, 1)[0].payload, b"record-1-success");
}

#[test]
fn test_sync_failure_fences_writer_and_rejects_subsequent_appends() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    log.append(1, "audit-class", 1, b"baseline-0".to_vec())
        .unwrap();

    // Inject sync_all failure
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::Sync, 0);
    log.set_storage_writer(Box::new(fault_writer));

    // Initial append fails with IO error
    let err = log
        .append(1, "audit-class", 1, b"unconfirmed-record".to_vec())
        .unwrap_err();
    assert!(matches!(err, LogError::Io(_)));

    // Writer must now be fenced due to uncertain hardware durability
    assert!(log.is_fenced());
    assert!(log.fenced_reason().is_some());
    assert_eq!(log.next_offset(), 1);
    assert_eq!(log.high_watermark(), 1);

    // Subsequent appends MUST be rejected with WriterFenced
    let sub_err = log
        .append(1, "audit-class", 1, b"subsequent-record".to_vec())
        .unwrap_err();
    assert!(
        matches!(sub_err, LogError::WriterFenced(_)),
        "Expected WriterFenced, got {:?}",
        sub_err
    );

    // Committed records up to HWM remain readable even when fenced
    let committed = log.read(0, 10);
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].payload, b"baseline-0");

    // Reopen clears fence and recovers exact pre-failure state (drop previous writer lock first)
    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert!(!reopened.is_fenced());
    assert_eq!(reopened.next_offset(), 1);
    assert_eq!(reopened.high_watermark(), 1);
    assert_eq!(reopened.read(0, 10).len(), 1);
    assert_eq!(reopened.read(0, 1)[0].payload, b"baseline-0");
}

#[test]
fn test_truncation_repair_failure_fences_writer() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    log.append(1, "audit-class", 1, b"baseline-0".to_vec())
        .unwrap();

    // Inject TruncationRepair failure (initial write fails, then set_len fails)
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::TruncationRepair, 5);
    log.set_storage_writer(Box::new(fault_writer));

    let err = log
        .append(1, "audit-class", 1, b"unrepairable-append".to_vec())
        .unwrap_err();
    assert!(
        matches!(err, LogError::WriterFenced(_)),
        "Expected WriterFenced when truncation repair fails, got {:?}",
        err
    );

    assert!(log.is_fenced());

    // Subsequent append rejected
    let sub_err = log
        .append(1, "audit-class", 1, b"blocked-append".to_vec())
        .unwrap_err();
    assert!(matches!(sub_err, LogError::WriterFenced(_)));

    // Reads still succeed for committed records
    assert_eq!(log.read(0, 10).len(), 1);

    // Restart recovery detects the torn write tail on disk and recovers clean baseline (drop previous writer lock first)
    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert_eq!(reopened.next_offset(), 1);
    assert_eq!(reopened.high_watermark(), 1);
    assert_eq!(reopened.read(0, 1)[0].payload, b"baseline-0");
}

#[test]
fn test_repair_sync_failure_fences_writer() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    log.append(1, "audit-class", 1, b"baseline-0".to_vec())
        .unwrap();

    // Inject RepairSync failure (initial write fails, set_len succeeds, sync_all fails)
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::RepairSync, 4);
    log.set_storage_writer(Box::new(fault_writer));

    let err = log
        .append(1, "audit-class", 1, b"unsynced-repair".to_vec())
        .unwrap_err();
    assert!(
        matches!(err, LogError::WriterFenced(_)),
        "Expected WriterFenced when repair sync fails, got {:?}",
        err
    );

    assert!(log.is_fenced());

    let sub_err = log
        .append(1, "audit-class", 1, b"blocked".to_vec())
        .unwrap_err();
    assert!(matches!(sub_err, LogError::WriterFenced(_)));

    // Reopen recovers cleanly (drop previous writer lock first)
    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert_eq!(reopened.next_offset(), 1);
    assert_eq!(reopened.high_watermark(), 1);
    assert_eq!(reopened.read(0, 10).len(), 1);
}

#[test]
fn test_epoch_advancement_preserved_on_failed_append() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    assert_eq!(log.current_epoch(), 1);

    // Inject length-write failure for an append requesting epoch 5
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::LengthWrite, 0);
    log.set_storage_writer(Box::new(fault_writer));

    let err = log
        .append(5, "audit-class", 1, b"epoch-5-failing".to_vec())
        .unwrap_err();
    assert!(matches!(err, LogError::Io(_)));

    // Epoch advancement must be preserved: partition epoch is now 5!
    assert_eq!(log.current_epoch(), 5);

    // Stale epoch (e.g. 3 < 5) must be rejected
    let stale_err = log
        .append(3, "audit-class", 1, b"stale-write".to_vec())
        .unwrap_err();
    assert!(matches!(
        stale_err,
        LogError::StaleEpoch {
            current: 5,
            requested: 3
        }
    ));

    // Writing at current epoch 5 succeeds
    let o0 = log
        .append(5, "audit-class", 1, b"valid-epoch-5".to_vec())
        .unwrap();
    assert_eq!(o0, 0);
    assert_eq!(log.next_offset(), 1);
    assert_eq!(log.high_watermark(), 1);
    assert_eq!(log.read(0, 1)[0].epoch, 5);
}

#[test]
fn test_reads_and_read_range_serve_committed_records_when_fenced() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    for i in 0..5 {
        log.append(1, "class-a", 1, format!("rec-{}", i).into_bytes())
            .unwrap();
    }
    assert_eq!(log.high_watermark(), 5);

    // Fence writer manually
    log.fence("Administrative quarantine for disk maintenance");
    assert!(log.is_fenced());

    // Reads continue to serve all 5 committed records without error
    let records = log.read(0, 10);
    assert_eq!(records.len(), 5);
    for (i, r) in records.iter().enumerate() {
        assert_eq!(r.base_offset, i as u64);
        assert_eq!(r.payload, format!("rec-{}", i).into_bytes());
    }

    // Range reads work
    let slice = log.read_range(1, 4);
    assert_eq!(slice.len(), 3);
    assert_eq!(slice[0].base_offset, 1);
    assert_eq!(slice[2].base_offset, 3);

    // New appends are rejected
    let append_err = log
        .append(1, "class-a", 1, b"fenced-reject".to_vec())
        .unwrap_err();
    assert!(matches!(append_err, LogError::WriterFenced(_)));
}

#[test]
fn test_sync_failure_state_a_record_persisted_on_disk_recovers_and_deduplicates() {
    use kamish_log::{RecordContext, SessionManager};
    use std::io::Write;

    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    // 1. Commit baseline record 0
    let ctx0 = RecordContext {
        producer_id: 100,
        producer_epoch: 1,
        sequence: 0,
        origin_region: "eu-west".into(),
        request_policy: "strict".into(),
        replication_targets: vec!["us-east".into()],
        source_generation: 1,
    };
    let o0 = log
        .append_with_context(1, "orders", 1, b"order-0".to_vec(), Some(ctx0))
        .unwrap();
    assert_eq!(o0, 0);

    // 2. Simulate State A:
    // Append record 1, where all bytes are written to disk, but sync_all fails,
    // and truncation rollback fails (or power was lost before rollback could occur).
    // The valid, un-synced envelope remains physically intact on disk.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::Sync, 0);
    log.set_storage_writer(Box::new(fault_writer));

    let ctx1 = RecordContext {
        producer_id: 100,
        producer_epoch: 1,
        sequence: 1,
        origin_region: "eu-west".into(),
        request_policy: "strict".into(),
        replication_targets: vec!["us-east".into()],
        source_generation: 1,
    };

    let err = log
        .append_with_context(
            1,
            "orders",
            1,
            b"order-1-unsynced".to_vec(),
            Some(ctx1.clone()),
        )
        .unwrap_err();
    assert!(matches!(err, LogError::Io(_)));

    // Invariants on live instance:
    // - Writer must be fenced!
    assert!(log.is_fenced());
    // - In-memory next_offset and HWM were NOT advanced (no premature ack)
    assert_eq!(log.next_offset(), 1);
    assert_eq!(log.high_watermark(), 1);
    // - Subsequent writes rejected
    assert!(matches!(
        log.append(1, "orders", 1, b"blocked".to_vec()).unwrap_err(),
        LogError::WriterFenced(_)
    ));

    // For State A: write the full valid envelope to disk to simulate that
    // the OS/controller flushed it to disk before the crash.
    // (We construct the valid envelope and write it directly to partition.data)
    {
        let mut envelope =
            kamish_log::RecordEnvelope::new(1, "orders", 1, 1, b"order-1-unsynced".to_vec());
        envelope.magic = kamish_log::RECORD_MAGIC_V2;
        envelope.context = Some(ctx1.clone());
        let bytes = serde_json::to_vec(&(
            envelope.magic,
            envelope.epoch,
            &envelope.class_id,
            envelope.policy_version,
            envelope.base_offset,
            &envelope.payload,
            &ctx1,
        ))
        .unwrap();
        envelope.checksum = crc32fast::hash(&bytes);
        let serialized = serde_json::to_vec(&envelope).unwrap();
        let len_bytes = (serialized.len() as u32).to_be_bytes();

        let mut raw_file = OpenOptions::new()
            .append(true)
            .open(dir.join("partition.data"))
            .unwrap();
        raw_file.write_all(&len_bytes).unwrap();
        raw_file.write_all(&serialized).unwrap();
        raw_file.flush().unwrap();
        raw_file.sync_all().unwrap();
    }

    // 3. Restart recovery: PartitionLog::open validates the complete segment (drop previous writer lock first)
    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert_eq!(reopened.high_watermark(), 2);
    assert_eq!(reopened.next_offset(), 2);

    let recovered_records = reopened.read(0, 10);
    assert_eq!(recovered_records.len(), 2);
    assert_eq!(recovered_records[0].base_offset, 0);
    assert_eq!(recovered_records[1].base_offset, 1);
    assert_eq!(recovered_records[1].payload, b"order-1-unsynced");
    assert!(recovered_records[1].verify_checksum());

    // 4. Session deduplication:
    // When the producer retries sequence 1 (which received an error during the sync failure),
    // SessionManager initialized from the recovered WAL detects sequence 1 as a duplicate retry!
    let session_mgr = SessionManager::new();
    for rec in &recovered_records {
        if let Some(ref ctx) = rec.context {
            session_mgr.update(
                ctx.producer_id,
                ctx.producer_epoch,
                ctx.sequence,
                rec.base_offset,
            );
        }
    }

    // Producer retry of seq 1:
    let dedup_result = session_mgr.check_sequence(100, 1, 1).unwrap();
    assert_eq!(
        dedup_result,
        Some(1),
        "State A recovery must allow idempotent deduplication on retry without duplicate append"
    );
}

#[test]
fn test_sync_failure_state_b_record_torn_on_disk_truncates_and_recovers() {
    use std::io::Write;

    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    let o0 = log
        .append(1, "audit-class", 1, b"baseline-0".to_vec())
        .unwrap();
    assert_eq!(o0, 0);
    let baseline_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();

    // Simulate State B: sync failure where write was incomplete and left torn bytes at tail
    {
        let mut file = OpenOptions::new()
            .append(true)
            .open(dir.join("partition.data"))
            .unwrap();
        // 4 bytes indicating length of 100, but only 12 bytes of torn payload follow
        file.write_all(&100u32.to_be_bytes()).unwrap();
        file.write_all(b"torn-partial").unwrap();
        file.flush().unwrap();
    }
    assert!(std::fs::metadata(dir.join("partition.data")).unwrap().len() > baseline_len);

    // Restart recovery: PartitionLog::open detects EOF torn write and cleanly truncates (drop previous writer lock first)
    drop(log);
    let reopened = PartitionLog::open(dir, 1).unwrap();
    assert_eq!(reopened.high_watermark(), 1);
    assert_eq!(reopened.next_offset(), 1);

    // Segment length is restored exactly to baseline
    let current_len = std::fs::metadata(dir.join("partition.data")).unwrap().len();
    assert_eq!(current_len, baseline_len);

    // Producer retry now succeeds cleanly at offset 1
    let o1 = reopened
        .append(1, "audit-class", 1, b"retried-order-1".to_vec())
        .unwrap();
    assert_eq!(o1, 1);
    assert_eq!(reopened.high_watermark(), 2);
    assert_eq!(reopened.next_offset(), 2);
}

#[test]
fn test_all_append_entry_points_honour_fence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();

    log.append(1, "audit", 1, b"valid".to_vec()).unwrap();
    log.fence("explicit administrative fence");
    assert!(log.is_fenced());
    assert_eq!(
        log.fenced_reason(),
        Some("explicit administrative fence".into())
    );

    // 1. append() honours fence
    let err1 = log.append(1, "audit", 1, b"payload1".to_vec()).unwrap_err();
    assert!(matches!(err1, LogError::WriterFenced(_)));

    // 2. append_with_context() honours fence
    let err2 = log
        .append_with_context(1, "audit", 1, b"payload2".to_vec(), None)
        .unwrap_err();
    assert!(matches!(err2, LogError::WriterFenced(_)));
}

#[test]
fn test_failed_repair_permanently_fences_live_instance_and_all_clones() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let log = PartitionLog::open(dir, 1).unwrap();
    let clone1 = log.clone();
    let clone2 = log.clone();

    // Write baseline record
    clone1.append(1, "audit", 1, b"rec-0".to_vec()).unwrap();

    // Inject unrepairable failure on log
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("partition.data"))
        .unwrap();
    let fault_writer = FaultInjectingWriter::new(file, FaultInjectionPoint::TruncationRepair, 4);
    log.set_storage_writer(Box::new(fault_writer));

    let err = log.append(1, "audit", 1, b"fail".to_vec()).unwrap_err();
    assert!(matches!(err, LogError::WriterFenced(_)));

    // Every clone must immediately observe the fence
    assert!(log.is_fenced());
    assert!(clone1.is_fenced());
    assert!(clone2.is_fenced());

    // Writes through any clone must fail permanently
    assert!(matches!(
        clone1
            .append(1, "audit", 1, b"attempt-clone1".to_vec())
            .unwrap_err(),
        LogError::WriterFenced(_)
    ));
    assert!(matches!(
        clone2
            .append(1, "audit", 1, b"attempt-clone2".to_vec())
            .unwrap_err(),
        LogError::WriterFenced(_)
    ));
}

#[test]
fn test_reopen_validates_complete_segment_before_permitting_writes() {
    use std::io::{Read, Seek, SeekFrom, Write};

    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let data_path = dir.join("partition.data");

    // Commit 3 valid records
    {
        let log = PartitionLog::open(dir, 1).unwrap();
        log.append(1, "audit", 1, b"record-0".to_vec()).unwrap();
        log.append(1, "audit", 1, b"record-1".to_vec()).unwrap();
        log.append(1, "audit", 1, b"record-2".to_vec()).unwrap();
    }

    // Corrupt a byte in the middle of record 1
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&data_path)
            .unwrap();
        file.seek(SeekFrom::Start(45)).unwrap();
        let mut b = [0u8; 1];
        file.read_exact(&mut b).unwrap();
        b[0] ^= 0xAA;
        file.seek(SeekFrom::Start(45)).unwrap();
        file.write_all(&b).unwrap();
        file.flush().unwrap();
    }

    // Reopen MUST fail loudly and refuse to clear/allow writes
    let open_res = PartitionLog::open(dir, 1);
    assert!(
        open_res.is_err(),
        "Corrupted segment must be rejected upon reopen"
    );
}
