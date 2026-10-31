# Milestone 2 — Formal Correctness, Invariants & Security Audit

**Project:** HydraStream / Kamish Distributed Event Streaming Engine  
**Repository:** `E:\Kamish`  
**Audit Scope:** Core engine invariants `INV-1` through `INV-8`, durability boundaries, policy enforcement, and adversarial failure modes  
**Audit Status:** COMPLETE & VERIFIED  

---

## 1. Executive Correctness Summary

Every core system invariant specified in the HydraStream engineering blueprint has been implemented, validated, and verified with direct automated tests.

| Invariant | Description | Governing Crate | Verification Status | Direct Test Evidence |
|---|---|---|---|---|
| **INV-1** | Monotonic Dense Sequence Numbering | `kamish-log` | **PASS / EXECUTED** | `test_monotonic_offset_and_crc_verification` |
| **INV-2** | Hardware `fsync` Durability Before ACK | `kamish-log` | **PASS / EXECUTED** | `test_fsync_flush_durability_and_sequential_reads` |
| **INV-3** | Bounded Outbox & Link Backpressure | `kamish-outbox`, `kamish-agent` | **PASS / EXECUTED** | `test_bounded_replication_queue_and_backpressure` |
| **INV-4** | High-Water Mark (HWM) Read Isolation | `kamish-log` | **PASS / EXECUTED** | `test_monotonic_offset_and_crc_verification` |
| **INV-5** | Signed Policy Governance & Audit Hash Chain | `kamish-policy`, `kamish-audit` | **PASS / EXECUTED** | `test_policy_signing_and_verification`, `test_enforce_e1_to_e10` |
| **INV-6** | Idempotent Session Deduplication | `kamish-log`, `kamish-agent` | **PASS / EXECUTED** | `test_idempotent_session_deduplication`, `test_offset_cursor_synchronization_and_deduplication_on_reconnect` |
| **INV-7** | Epoch Writer Fencing & Suffix Quarantine | `kamish-own`, `kamish-meta` | **PASS / EXECUTED** | `test_quarantine_superseded_records`, `test_stale_epoch_rejection` |
| **INV-8** | Zero Cross-Border Plaintext & Key Denial | `kamish-seal`, `kamish-checkers` | **PASS / EXECUTED** | `test_sealed_encryption_and_cross_border_key_denial`, `test_canary_scanner_detects_leak` |

---

## 2. Invariant-by-Invariant Deep Technical Audit

### 2.1 INV-1: Monotonic Dense Sequence Numbering per Partition
- **Implementation:** Defined in `crates/kamish-log/src/lib.rs`.
- **Mechanism:** Within `PartitionLog::append`, offset reservation occurs under an in-memory lock (`inner.next_offset`). The writer lock serializes all appends. Each envelope commits offset $N$, and the subsequent append is strictly guaranteed to be $N + 1$.
- **Adversarial Edge Case Handling:** In `PartitionLog::open`, segment recovery iterates records and enforces `envelope.base_offset == next_offset`. If non-monotonic or missing records occur, `LogError::NonMonotonicOffset` is returned.
- **Verification:** `OrderingChecker::verify_partition_records` explicitly fails if any gap occurs in a sequence batch.

### 2.2 INV-2: Hardware Durability Flush (`fsync`) Before Acknowledgment
- **Implementation:** `crates/kamish-log/src/lib.rs` (lines 200–232).
- **Mechanism:**
  ```rust
  let file = file_guard.as_mut().unwrap();
  file.write_all(&len_bytes)?;
  file.write_all(&serialized)?;
  file.flush()?;
  file.sync_all()?; // POSIX fsync directly to persistent disk hardware
  ```
  The broker only advances the in-memory committed log and acknowledges the produce request to the client AFTER `file.sync_all()` completes successfully.
- **Decoupled Concurrency:** File disk I/O and `sync_all` are guarded by `writer_lock: Arc<Mutex<Option<File>>>`, completely independent of the reader lock `inner.read()`. Concurrent reads are never blocked while a disk write flushes.
- **Crash Recovery & Torn Write Protection:** In `PartitionLog::open`, if a crash occurs mid-write, partial trailing bytes at the segment tail are detected via `bincode_deserialize_envelope` error. The segment is automatically truncated using `file.set_len(valid_file_len)` back to the last valid CRC32 envelope, preventing segment corruption.

### 2.3 INV-3: Bounded Outbox Capacity & Backpressure
- **Implementation:** `crates/kamish-outbox/src/lib.rs` and `cmd/kamish-agent/src/lib.rs`.
- **Mechanism:** The outbox tracks `max_messages` and `max_bytes`. When the outbox memory threshold is reached, `enqueue` returns `Err(OutboxError::CapacityExceeded)`. In `kamish-agent`, bounded `tokio::sync::mpsc::channel(queue_capacity)` creates natural async backpressure against the broker outbox polling task when replication links slow down.
- **Verification:** Verified in `test_outbox_bounded_capacity` and `test_bounded_replication_queue_and_backpressure`.

### 2.4 INV-4: Strict High-Water Mark Read Visibility
- **Implementation:** `crates/kamish-log/src/lib.rs` (`PartitionLog::read` and `PartitionLog::read_range`).
- **Mechanism:** Readers can only retrieve offsets where $\text{offset} < \text{high\_watermark}$. Uncommitted appends or pending batches are invisible to consumers.
- **Verification:** `PartitionLog::read(start_offset, max_records)` caps the returned records to `std::cmp::min(max_records, high_watermark - start_offset)`.

### 2.5 INV-5: Cryptographic Policy Governance & Hash-Chained Audit Trail
- **Implementation:** `crates/kamish-policy/src/lib.rs` and `crates/kamish-audit/src/lib.rs`.
- **Mechanism:** Policies are structured as JSON documents signed with Ed25519 asymmetric cryptographic key pairs. Rollback to older policy versions is cryptographically rejected (`PolicyError::PolicyRollbackRejected`).
- **Audit Ledger:** Every admission or replication denial generates an immutable `AuditEntry` with a SHA-256 hash linking to the preceding entry:
  $$\text{hash}_n = \text{SHA256}(\text{seq}_n \parallel \text{timestamp}_n \parallel \text{hash}_{n-1} \parallel \text{action}_n \parallel \text{class\_id}_n \parallel \text{outcome}_n \parallel \text{reason}_n)$$
- **Tamper Evidence:** Any modification to an audit log entry invalidates the chain from that point forward.

### 2.6 INV-6: Idempotent Session Sequence Deduplication
- **Implementation:** `crates/kamish-log/src/lib.rs` (`SessionManager`) and `cmd/kamish-agent/src/lib.rs` (`handshake_and_sync_cursor`).
- **Mechanism:** The broker tracks `(producer_id, producer_epoch, last_seq, last_offset)`.
  - When $seq == last\_seq$: The broker treats the request as an idempotent retry and immediately returns the cached `last_offset` without re-appending to disk.
  - When $seq == last\_seq + 1$: The broker processes the message and updates session state.
  - When $seq > last\_seq + 1$: Out-of-order sequence error is returned.
- **Agent Reconnect Handshake:** When an inter-region agent reconnects, it queries the destination broker's durable watermark via `GetWatermark`. Batches already persisted on the receiver are deduplicated with `idempotent_duplicate_noop`.

### 2.7 INV-7: Epoch Writer Fencing & Divergent Suffix Quarantine
- **Implementation:** `crates/kamish-own/src/lib.rs` and `crates/kamish-meta/src/lib.rs`.
- **Mechanism:**
  - **Writer Fencing:** Every partition write requires an `epoch`. If an incoming append carries an epoch lower than the metadata quorum's current epoch, it is rejected with `OwnershipError::StaleWriterFenced`.
  - **Lease Expiry:** Leases expire deterministically. Re-promotion before lease expiry fails unless held by the current owner.
  - **Quarantine Store:** If an isolated leader accepts writes during a network partition and later reconnects to find a higher epoch has been elected, `reconcile_divergent_suffix` truncates the uncommitted suffix and archives it into `QuarantineStore`, preventing silent data loss while preserving linear history.

### 2.8 INV-8: Zero Cross-Border Plaintext & Regional Key Custody Denial
- **Implementation:** `crates/kamish-seal/src/lib.rs` and `crates/kamish-checkers/src/lib.rs`.
- **Mechanism:** Sensitive payloads (`SealedEnvelope`) are encrypted using regional key identifiers. If an unauthorized node outside the data residency boundary attempts to unseal the payload, `RegionalKeyService::get_key` denies access with `SealError::CrossBorderKeyAccessDenied`.
- **Canary Auditing:** `CanaryScanner` actively inspects cross-border byte buffers for prohibited tokens, flagging violations instantly if regional barriers are breached.

---

## 3. Adversarial Edge Case Verification

1. **Power Outage During Fsync (Torn Writes):**
   - *Test:* `test_torn_write_tail_truncation_recovery` in `kamish-log`.
   - *Result:* Truncated records at segment tail are automatically clipped to the last valid CRC32 boundary; clean recovery verified across restarts.
2. **Slow-Link Network Congestion:**
   - *Test:* `test_bounded_replication_queue_and_backpressure` and `test_scenario_b_slow_link_and_backpressure_demo`.
   - *Result:* Outbox gracefully applies backpressure; agent retry worker employs exponential backoff without crashing or leaking memory.
3. **Partition Ownership Split-Brain:**
   - *Test:* `test_promotion_before_lease_expires_fails` and `test_quarantine_superseded_records` in `kamish-own`.
   - *Result:* Split-brain write attempts are rejected; superseded diverging branches are safely quarantined.

