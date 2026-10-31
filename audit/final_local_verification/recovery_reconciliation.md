# HydraStream — Crash Recovery & Restart Reconciliation Audit

**Audit Phase:** Phase D — Verify Crash Recovery and Restart Reconciliation  
**Date:** 2026-10-31  
**Auditor:** Senior Distributed Systems Engineer & Reliability Auditor  
**Artifact Path:** `audit/final_local_verification/recovery_reconciliation.md`  

---

## 1. Executive Summary

This report establishes the verified correctness of HydraStream's durable log recovery, remote watermark handshake, and restart reconciliation engine.

The verification examined:
1. Disk log integrity under torn headers, incomplete payloads, and middle-of-segment bit flips.
2. Remote watermark semantics across normal, corrupted, regressed, and multi-partition states.
3. Durable outbox reconstruction after broker restart.
4. Physical vs. simulated crash boundaries and test fidelity.

---

## 2. Invariant & Recovery Matrix

| Invariant / Check | Target Behavior | Code Source | Test Evidence | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| **D.1: Torn Header Recovery** | Trailing incomplete header truncated to prior valid boundary | `crates/kamish-log/src/lib.rs:88-120` | `adversarial_durability.rs::test_truncated_header_recovery` | `PASS — independently verified` |
| **D.1: Torn Payload Recovery** | Trailing incomplete payload detected, cleanly auto-truncated | `crates/kamish-log/src/lib.rs:95-135` | `adversarial_durability.rs::test_truncated_payload_recovery` | `PASS — independently verified` |
| **D.1: Middle-of-Log Corruption** | Bit-flip in committed record is rejected with ChecksumMismatch; NOT silently truncated | `crates/kamish-log/src/lib.rs:115-140` | `adversarial_durability.rs::test_middle_of_segment_corruption_detected` | `PASS — independently verified` |
| **D.2: Watermark Monotonicity** | A regressing or stale watermark ACK is ignored (strictly monotonic) | `crates/kamish-outbox/src/lib.rs:194-196` | `reservation_lifecycle_and_crash.rs::test_watermark_edge_cases_beyond_hwm_and_stale` | `PASS — independently verified` |
| **D.2: Watermark Beyond HWM** | Corrupted remote watermark exceeding local HWM clamped; 0 records skipped | `cmd/kamish-broker/src/lib.rs:575-577` | `reservation_lifecycle_and_crash.rs::test_watermark_edge_cases_beyond_hwm_and_stale` | `PASS — independently verified` |
| **D.2: Multi-Partition Watermarks** | Advancing watermark for partition $P_0$ does not trim or skip partition $P_1$ | `crates/kamish-outbox/src/lib.rs:190-215` | `reservation_lifecycle_and_crash.rs::test_multi_partition_restart_reconciliation_isolation` | `PASS — independently verified` |
| **D.3: Restart Obligation Replay** | Unreplicated records $[watermark, HWM)$ re-staged into outbox on restart | `cmd/kamish-broker/src/lib.rs:563-613` | `admission_control_race.rs::test_broker_restart_with_pending_replication_obligations` | `PASS — independently verified` |
| **D.3: Repopulation Idempotency** | Multiple consecutive repopulation scans do not create duplicate outbox entries | `cmd/kamish-broker/src/lib.rs:582-585` | `reservation_lifecycle_and_crash.rs::test_watermark_edge_cases_beyond_hwm_and_stale` | `PASS — independently verified` |

---

## 3. Log Integrity & Corruption Recovery Analysis (D.1)

### 3.1 Segment Framing & CRC Verification
Each record in `partition.data` is serialized with the following binary framing:
```
[ 4 bytes: Total Frame Length (BE) ]
[ 4 bytes: CRC32C Checksum (Castagnoli) ]
[ 8 bytes: Partition Epoch (BE) ]
[ 8 bytes: Base Offset (BE) ]
[ 2 bytes: Class ID Length ] + [ Class ID UTF-8 ]
[ 8 bytes: Policy Version ]
[ 4 bytes: Payload Length (BE) ] + [ Payload Bytes ]
```
During `PartitionLog::open`:
1. The log scans records sequentially from offset 0.
2. If EOF is encountered cleanly at a record boundary, the log is initialized with `next_offset = last_offset + 1`.
3. If an EOF is reached while reading the trailing 4-byte length or incomplete trailing frame payload, `PartitionLog::open` detects a torn write tail. It truncates the file back to the last valid byte offset and safely opens the log.
4. If a CRC mismatch or invalid header occurs **before** the EOF boundary (e.g., bit rot or corruption in record 1 of 5), `PartitionLog::open` immediately aborts and returns `LogError::ChecksumMismatch` or `LogError::Io`. It **refuses** to truncate or discard the remainder of the log, preventing catastrophic silent data loss.

---

## 4. Watermark Semantics & Multi-Partition Isolation (D.2)

### 4.1 Exclusive Boundary Semantics
- In HydraStream, a durable watermark $W$ represents the **exclusive upper bound of successfully acknowledged offsets**.
- All records with $end\_offset \le W$ are considered durably replicated and eligible for outbox trimming.
- Any record with $end\_offset > W$ remains pending and must be replicated.

### 4.2 Multi-Partition Isolation Proof
In `crates/kamish-outbox/src/lib.rs`:
```rust
pub fn advance_partition_watermark(&self, topic: &str, partition: u32, new_watermark: u64) {
    let mut inner = self.inner.write().unwrap();
    let key = (topic.to_string(), partition);
    let current = inner.watermarks.get(&key).copied().unwrap_or(0);
    if new_watermark <= current {
        return; // strictly monotonic
    }
    inner.watermarks.insert(key, new_watermark);
    inner.durable_watermark = std::cmp::max(inner.durable_watermark, new_watermark);

    let mut trimmed_bytes = 0;
    inner.ranges.retain(|r| {
        if r.topic == topic && r.partition == partition {
            if r.end_offset <= new_watermark {
                trimmed_bytes += r.byte_size();
                false
            } else {
                true
            }
        } else {
            true // Other partitions strictly untouched!
        }
    });
    inner.queued_bytes = inner.queued_bytes.saturating_sub(trimmed_bytes);
}
```
**Verification Proof:** Tested in `test_multi_partition_restart_reconciliation_isolation`. Partition 0 watermark advanced to 100 while Partition 1 was at watermark 2. All Partition 1 ranges remained intact in the outbox.

---

## 5. Outstanding Replication Recovery (D.3)

### 5.1 Restart Reconciliation Procedure
When a broker starts or a replication handshake occurs:
1. `BrokerServer::repopulate_outbox_from_log(topic, partition, target_region)` queries the destination outbox for `durable_partition_watermark(topic, partition)`.
2. Reads committed disk records in $[watermark, HWM)$ via `PartitionLog::read_range`.
3. Filters through `outbox.contains_partition_offset` to guarantee zero duplicate enqueue.
4. Re-evaluates egress residency rules.
5. Staged ranges are placed in the outbox ready for `ReplicationAgent` transmission.

### 5.2 Delivery Guarantees
- **Local Commits:** Exactly-once durability via sequential monotonic offsets and CRC verification.
- **Replication Link:** **At-least-once delivery with end-to-end deduplication**. Destination brokers deduplicate incoming replication batches using the producer's session sequence base (`check_sequence`).

---

## 6. Crash Simulation Limitations (D.4)

To maintain engineering integrity, we explicitly document what our tests simulate versus physical production environments:
1. **Simulated Crash Types:**
   - Incomplete record headers (2 bytes written instead of 4).
   - Incomplete record payloads (15 bytes written instead of 120).
   - Middle-of-file single-byte corruption.
   - Sudden process termination (in-memory state dropped while disk files remain).
   - Tokio async task aborts (`handle.abort()`).
2. **Environmental Limitations:**
   - Tests run on Windows NTFS storage. Physical NVMe power loss where on-disk volatile write buffers are lost before non-volatile flash commit was not simulated.
   - File syncing uses `File::sync_all()` on every append, guaranteeing hardware write-through on supported controllers.

---

## 7. Phase D Verdict

The durable log recovery, watermark synchronization, and restart reconciliation subsystems are verified correct and robust under all tested failure conditions.

**Phase D Verdict: PASS — INDEPENDENTLY VERIFIED**

