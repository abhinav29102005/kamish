# Milestone 3 — Claims-to-Evidence Audit Matrix

**Project:** HydraStream / Kamish Distributed Event Streaming Engine  
**Working Repository:** `E:\Kamish`  
**Working Branch:** `main`  
**Toolchain:** `rustc 1.97.1 (8bab26f4f 2026-07-14)` / `cargo 1.97.1 (c980f4866 2026-06-30)`  
**Audit Date:** 2026-10-31  

---

## 1. Objective and Scope

This document provides a rigorous, adversarial claims-to-evidence matrix evaluating every significant claim made in the Milestone 2 documentation against the actual source code, unit/integration tests, and independently reproduced benchmarks.

Each claim is evaluated under four evidence classifications:
- **Code-Inspected:** Source logic verified by line-by-line static review.
- **Test-Verified:** Actively confirmed through reproducible, automated automated test execution (`cargo test`).
- **Benchmark-Measured:** Quantitatively validated under documented hardware and link profiles.
- **Unverified / Refuted:** Claim lacked evidence, exhibited edge-case defects under adversarial conditions, or was refuted.

---

## 2. Exhaustive Claims-to-Evidence Matrix

| # | System Guarantee / Milestone 2 Claim | Source Implementation | Supporting Test Cases | Audit Verdict | Remaining Uncertainty / Discovery | Next Test Required |
|---|---|---|---|---|---|---|
| 1 | **Torn-Write Recovery & Log Integrity:** Segment recovers cleanly from torn writes at segment tail. | `crates/kamish-log/src/lib.rs` (`PartitionLog::open`, `bincode_deserialize_envelope`) | `test_torn_write_tail_truncation_recovery`, `test_truncated_header_recovery`, `test_truncated_payload_recovery` | **Test-Verified** | **Defect Discovered & Fixed:** Earlier code silently truncated middle-of-segment corruption. Fixed to distinguish EOF torn writes from middle corruption (`UnexpectedEof` at EOF vs `InvalidData`). | `test_middle_of_segment_corruption_detected` (PASS) |
| 2 | **Monotonic Dense Offsets (`INV-1`):** Offsets strictly increase $0, 1, 2 \dots N$ without gaps or regressions. | `crates/kamish-log/src/lib.rs` (`PartitionLog::append`, `PartitionLog::open`), `crates/kamish-checkers/src/lib.rs` | `test_monotonic_offset_and_crc_verification`, `test_repeated_restarts_and_monotonicity`, `test_ordering_checker_detects_gap` | **Test-Verified** | None. Monotonicity preserved across repeated process crashes and restarts. | Ran repeated restart test across 5 restarts (50 records). |
| 3 | **Hardware Durability Flush (`INV-2`):** POSIX `fsync` (`sync_all()`) executes before write acknowledgment. | `crates/kamish-log/src/lib.rs` (`PartitionLog::append`, lines 235-245) | `test_fsync_flush_durability_and_sequential_reads`, `adversarial_durability.rs` | **Code-Inspected & Test-Verified** | POSIX `sync_all()` guarantees OS buffer flush to controller cache. Power-loss resilience requires non-volatile write cache. | Verified that log reopen after drop recovers all committed records with valid CRC32. |
| 4 | **Decoupled Lock Striping:** 32 partition stripes eliminate cross-partition lock contention. | `cmd/kamish-broker/src/lib.rs` (`StripedStore`, lines 150-185) | `test_concurrent_multi_partition_lock_striping_and_reader_decoupling` | **Benchmark-Measured & Test-Verified** | Hash distribution relies on `DefaultHasher`. Hash collisions possible if partition count $\gg 32$. | Multi-worker benchmark scaling up to 8 concurrent workers. |
| 5 | **Tokio Async Thread Protection:** Blocking disk I/O offloaded to `spawn_blocking`. | `cmd/kamish-broker/src/lib.rs` (`handle_connection`, lines 625-635) | `test_async_framed_tcp_listener_and_client`, `benchmark` | **Code-Inspected & Test-Verified** | Threadpool limit bounded by Tokio default (`512` blocking threads). | Monitored async TCP latency during heavy disk write load. |
| 6 | **Regional Data Residency (`INV-8`):** StrictLocal events blocked at ingress and egress. | `crates/kamish-enforce/src/lib.rs`, `cmd/kamish-broker/src/lib.rs` (`produce`, `handle_replication`), `crates/kamish-seal/src/lib.rs` | `test_unauthorized_destination_egress_denial`, `test_broker_strict_residency_rejection`, `test_sealed_encryption_and_cross_border_key_denial` | **Test-Verified** | Prohibited payloads cannot cross borders in cleartext. Sealed envelopes permit encrypted cross-border transit if authorized. | Red-team test suite executed 6 adversarial residency scenarios. |
| 7 | **Zero Audit Payload Leakage:** Audit chain contains no confidential payload data. | `crates/kamish-audit/src/lib.rs` (`AuditEntry`) | `test_audit_ledger_zero_payload_disclosure` | **Test-Verified** | Verified `AuditEntry` JSON serialization contains zero message body fields. | Inspect serialized JSON of audit entries. |
| 8 | **Replication Idempotency (`INV-6`):** Retried range execution is a no-op; no duplicate appends. | `cmd/kamish-broker/src/lib.rs` (`handle_replication`), `crates/kamish-log/src/lib.rs` (`SessionManager`) | `test_destination_persistence_followed_by_lost_ack`, `test_idempotent_session_deduplication` | **Test-Verified** | None. Re-sending an applied range returns `idempotent_duplicate_noop` with current watermark. | Simulated dropped ACK over replication link followed by duplicate retry. |
| 9 | **Watermark Synchronization:** Reconnecting agent synchronizes cursors via `GetWatermark`. | `cmd/kamish-agent/src/lib.rs` (`handshake_and_sync_cursor`) | `test_offset_cursor_synchronization_and_deduplication_on_reconnect` | **Test-Verified** | Outbox advances watermark and trims applied messages cleanly. | Reconnect handshake test with outbox trim assertion. |
| 10 | **Epoch Writer Fencing (`INV-7`):** Stale leader writes rejected; divergent suffixes quarantined. | `crates/kamish-own/src/lib.rs`, `crates/kamish-meta/src/lib.rs` (`compare_and_swap_owner`) | `test_stale_replication_worker_after_epoch_change`, `test_promotion_before_lease_expires_fails`, `test_quarantine_superseded_records` | **Test-Verified** | Metadata store uses lease duration; promotion before lease expiration is rejected. | Stale writer append test with epoch mismatch. |
| 11 | **Outbox Flow Control & Backpressure (`INV-3`):** Bounded queues prevent memory exhaustion. | `crates/kamish-outbox/src/lib.rs`, `cmd/kamish-broker/src/lib.rs` (`produce`) | `test_outbox_bounded_capacity`, `test_bounded_replication_queue_and_backpressure` | **Test-Verified** | **P0 Defect Discovered & Fixed:** `BrokerServer::produce` previously had `let _ = outbox.enqueue(range);` silently dropping messages when outbox was full. Fixed to pre-check outbox capacity and reject write with explicit backpressure error before appending. | Verified backpressure rejection and zero silent message drop. |
| 12 | **Reported Throughput & Latency:** 1 worker ~1,012 msg/s; 4 workers ~3,997 msg/s; 8 workers ~6,464 msg/s. | `cmd/kamish-ctl/src/main.rs` (`benchmark`) | In-tree benchmark harness with 500–4,000 records | **Benchmark-Measured** | Benchmark measures local durable persistence (`fsync`) + policy check + outbox staging, not 2-phase synchronous WAN replication commit. | Ran 1, 4, 8 worker benchmarks with BypassSandbox: true. |

---

## 3. Critical Adversarial Findings & Corrections

1. **Middle-of-Segment Corruption vs. Tail Truncation:**
   - *Previous Behavior:* Any deserialization failure in `PartitionLog::open` triggered tail truncation, silently deleting all records following a corrupted byte.
   - *Correction:* Differentiated `UnexpectedEof` occurring at the exact segment tail (torn write) from deserialization errors occurring with trailing bytes (middle corruption). Middle corruption now fails loudly with `LogError::Io` or `LogError::ChecksumMismatch`, preventing silent data loss.
2. **Silent Replication Data Loss on Outbox Saturation:**
   - *Previous Behavior:* In `BrokerServer::produce`, `let _ = outbox.enqueue(range);` ignored outbox capacity errors. If an outbox reached memory limits, messages were committed locally but never replicated, violating `project.md` §3.2.4.
   - *Correction:* Implemented outbox capacity pre-check in `BrokerServer::produce`. If any required destination outbox is full, the write is rejected with `sovereignty_allowed: false, denial_reason: "Backpressure: destination outbox for '...' full"` BEFORE appending to disk.
3. **Partial Overlap Replication Resynchronization:**
   - *Previous Behavior:* When an agent submitted a range that partially overlapped existing local records (e.g. from 1..3 when receiver was at offset 2), the receiver returned a generic error string.
   - *Correction:* Added explicit `partial_overlap` detection in `handle_replication` and updated `ReplicationAgent` to advance outbox watermarks to match receiver progress.

