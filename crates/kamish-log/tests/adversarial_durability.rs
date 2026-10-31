use kamish_log::{LogError, PartitionLog};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};

#[test]
fn test_truncated_header_recovery() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();
    let data_path = path.join("partition.data");

    // 1. Write 3 valid records
    {
        let log = PartitionLog::open(&path, 1).unwrap();
        log.append(1, "eu-sovereign", 1, b"msg-0".to_vec()).unwrap();
        log.append(1, "eu-sovereign", 1, b"msg-1".to_vec()).unwrap();
        log.append(1, "eu-sovereign", 1, b"msg-2".to_vec()).unwrap();
        assert_eq!(log.high_watermark(), 3);
    }
    let valid_len = std::fs::metadata(&data_path).unwrap().len();

    // 2. Simulate torn write during header write: append 2 bytes (out of 4 length bytes)
    {
        let mut file = OpenOptions::new().append(true).open(&data_path).unwrap();
        file.write_all(&[0x00, 0x01]).unwrap();
        file.flush().unwrap();
    }
    assert!(std::fs::metadata(&data_path).unwrap().len() > valid_len);

    // 3. Re-open log: it must detect the truncated header at tail, truncate back, and recover
    let recovered = PartitionLog::open(&path, 1).unwrap();
    assert_eq!(recovered.high_watermark(), 3);
    assert_eq!(recovered.next_offset(), 3);
    assert_eq!(std::fs::metadata(&data_path).unwrap().len(), valid_len);

    let records = recovered.read(0, 10);
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].base_offset, 0);
    assert_eq!(records[1].base_offset, 1);
    assert_eq!(records[2].base_offset, 2);

    // 4. Append next message
    let o3 = recovered
        .append(1, "eu-sovereign", 1, b"msg-3".to_vec())
        .unwrap();
    assert_eq!(o3, 3);
    assert_eq!(recovered.high_watermark(), 4);
}

#[test]
fn test_truncated_payload_recovery() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();
    let data_path = path.join("partition.data");

    // 1. Write 2 valid records
    {
        let log = PartitionLog::open(&path, 1).unwrap();
        log.append(1, "global-audit", 1, b"first-record".to_vec())
            .unwrap();
        log.append(1, "global-audit", 1, b"second-record".to_vec())
            .unwrap();
        assert_eq!(log.high_watermark(), 2);
    }
    let valid_len = std::fs::metadata(&data_path).unwrap().len();

    // 2. Simulate torn write during payload write: 4-byte len says 120 bytes, but only 15 bytes written
    {
        let mut file = OpenOptions::new().append(true).open(&data_path).unwrap();
        file.write_all(&120u32.to_be_bytes()).unwrap();
        file.write_all(b"partial-payload").unwrap();
        file.flush().unwrap();
    }
    assert!(std::fs::metadata(&data_path).unwrap().len() > valid_len);

    // 3. Re-open log: it must detect incomplete payload, truncate back to valid boundary
    let recovered = PartitionLog::open(&path, 1).unwrap();
    assert_eq!(recovered.high_watermark(), 2);
    assert_eq!(recovered.next_offset(), 2);
    assert_eq!(std::fs::metadata(&data_path).unwrap().len(), valid_len);

    // 4. Subsequent write cleanly appends at offset 2
    let o2 = recovered
        .append(1, "global-audit", 1, b"clean-third-record".to_vec())
        .unwrap();
    assert_eq!(o2, 2);
    assert_eq!(recovered.high_watermark(), 3);

    // 5. Reopen again, all 3 records intact
    let reopened = PartitionLog::open(&path, 1).unwrap();
    assert_eq!(reopened.high_watermark(), 3);
    assert_eq!(reopened.read(0, 10).len(), 3);
}

#[test]
fn test_middle_of_segment_corruption_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();
    let data_path = path.join("partition.data");

    // 1. Write 5 valid records
    {
        let log = PartitionLog::open(&path, 1).unwrap();
        for i in 0..5 {
            log.append(
                1,
                "class-a",
                1,
                format!("record-payload-{}", i).into_bytes(),
            )
            .unwrap();
        }
        assert_eq!(log.high_watermark(), 5);
    }

    // 2. Corrupt a byte in the middle of partition.data (within record 1 or 2)
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&data_path)
            .unwrap();
        // Seek into middle record (offset ~120 bytes)
        file.seek(SeekFrom::Start(120)).unwrap();
        let mut b = [0u8; 1];
        file.read_exact(&mut b).unwrap();
        b[0] ^= 0xFF; // Flip all bits
        file.seek(SeekFrom::Start(120)).unwrap();
        file.write_all(&b).unwrap();
        file.flush().unwrap();
    }

    // 3. Re-open log: it MUST NOT silently accept corrupted data!
    let res = PartitionLog::open(&path, 1);
    assert!(
        res.is_err(),
        "Middle-of-segment corruption must be rejected"
    );
    match res.unwrap_err() {
        LogError::ChecksumMismatch {
            offset,
            computed,
            recorded,
        } => {
            println!(
                "Correctly detected ChecksumMismatch at offset {}: computed={}, recorded={}",
                offset, computed, recorded
            );
            assert_ne!(computed, recorded);
        }
        LogError::Io(e) => {
            println!(
                "Detected deserialization IO error due to corrupted frame: {}",
                e
            );
        }
        other => {
            println!("Detected log error: {:?}", other);
        }
    }
}

#[test]
fn test_repeated_restarts_and_monotonicity() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();

    let mut expected_offset = 0;
    for restart in 0..5 {
        let log = PartitionLog::open(&path, 1).unwrap();
        assert_eq!(
            log.next_offset(),
            expected_offset,
            "Restart {} had wrong starting offset",
            restart
        );
        assert_eq!(log.high_watermark(), expected_offset);

        for _ in 0..10 {
            let off = log
                .append(
                    1,
                    "stream-class",
                    1,
                    format!("msg-{}", expected_offset).into_bytes(),
                )
                .unwrap();
            assert_eq!(off, expected_offset);
            expected_offset += 1;
        }
    }

    // Final reopen: verify all 50 records exist in dense order 0..49
    let final_log = PartitionLog::open(&path, 1).unwrap();
    assert_eq!(final_log.high_watermark(), 50);
    assert_eq!(final_log.next_offset(), 50);

    let records = final_log.read(0, 100);
    assert_eq!(records.len(), 50);
    for (i, r) in records.iter().enumerate() {
        assert_eq!(r.base_offset, i as u64);
        assert!(r.verify_checksum());
    }
}
