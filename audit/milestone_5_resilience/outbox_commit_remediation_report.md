# HydraStream — Remediation Report: Outbox Reservation Commit Consistency & WAL Integrity

**Deliverable ID:** M5-D1-REMEDIATION  
**Milestone:** 5 — Adversarial Resilience & Defect Remediation  
**Repository:** `E:\Kamish` (`main` branch)  
**Baseline Commit:** `77bb051bfa77e6be80e8d8e5f4ea3cd4dad9eba2`  
**Date:** 2026-11-01  
**Author:** Distributed Systems Reliability Auditor & Core Engineer  
**Resolution Status:** **VERIFIED FIXED — 95/95 TESTS PASSING (0 FAILURES, 0 CLIPPY WARNINGS)**

---

## 1. Root Cause Established from Source Inspection

The investigated defect involved a critical consistency hazard between the local write-ahead log (`PartitionLog`) and the replication queue (`DestinationOutbox`):

1. **Check-Then-Grow Disconnect in Admission Control:**
   In [`cmd/kamish-broker/src/lib.rs`](file:///E:/Kamish/cmd/kamish-broker/src/lib.rs), `BrokerServer::produce_at_epoch` reserved an estimated capacity before disk append:
   ```rust
   let estimated_range_size = req.payload.len() + 64;
   match outbox.reserve_capacity(estimated_range_size) { ... }
   ```
2. **Commit Capacity Growth Check:**
   In [`crates/kamish-outbox/src/lib.rs`](file:///E:/Kamish/crates/kamish-outbox/src/lib.rs), `OutboxReservation::commit` reconciled actual range size against `reserved_bytes`:
   ```rust
   if actual_size > self.reserved_bytes {
       let additional = actual_size - self.reserved_bytes;
       if additional > inner.max_bytes.saturating_sub(inner.queued_bytes) {
           return Err(OutboxError::QueueCapacityExceeded { ... });
       }
       inner.queued_bytes += additional;
   }
   ```
3. **Post-Append Failure Window & Early Exit:**
   In `BrokerServer::produce_at_epoch`, the local log append (`log.append_with_context`) and producer session update (`session_mgr.update`) occurred **prior to** calling `reservation.commit(range)`:
   ```rust
   for (_peer, reservation, is_sealed) in reservations {
       let range = OutboxRange { ... };
       reservation.commit(range)?; // <--- Early exit on commit growth failure!
   }
   ```
4. **Failure Outcome:**
   - If `actual_size > self.reserved_bytes` and the outbox became saturated, `commit()` returned `Err(OutboxError::QueueCapacityExceeded)`.
   - The uncommitted reservation's `Drop` implementation safely refunded `reserved_bytes`, but the range was **never enqueued into `outbox.ranges`**.
   - The loop exited early via `?`, failing to commit any remaining peer destinations in multi-destination pipelines.
   - The broker returned an error to the producer, **despite the record having already been committed to the WAL on disk**.
   - If the producer retried, `session_mgr.check_sequence` matched the cached sequence and returned `idempotent_duplicate` with an ACK, masking the fact that the in-memory outbox was empty and the replication obligation was dropped in memory.

---

## 2. Reproduction Test and Original Failure

A minimal, deterministic reproduction was authored in [`cmd/kamish-broker/tests/outbox_commit_resilience.rs`](file:///E:/Kamish/cmd/kamish-broker/tests/outbox_commit_resilience.rs) under `test_record_size_exceeding_estimate_retains_durable_wal_obligation`:

1. **Initial Setup:**
   An outbox was configured with `max_bytes = 120`.
2. **Fault Injection:**
   - An initial reservation of 50 bytes was claimed (simulating an underestimated reservation).
   - A competing producer claimed 50 bytes (`queued_bytes = 100`, leaving 20 bytes available).
   - A 100-byte record (payload 36 bytes + 64 bytes metadata) was appended to the local WAL at offset 0.
   - At commit time, `actual_size` (100 bytes) exceeded `reserved_bytes` (50 bytes) by 50 bytes.
   - Because only 20 bytes remained available, `reservation.commit` returned:
     ```
     Err(OutboxError::QueueCapacityExceeded { current: 100, max: 120 })
     ```
3. **Original Flaw Observed:**
   Under the original broker code, `reservation.commit(range)?;` threw an error and abandoned the produce path, leaving the durable WAL record untracked by the active in-memory replication agent.

---

## 3. Chosen Remediation and Rationale

We implemented a defense-in-depth remediation preserving all bounded outbox invariants, zero-oversubscription guarantees, and regional residency policies without weakening assertions:

### Pillar 1: Exact Required Sizing Before Append (Admission Pre-Allocation)
- Introduced `OutboxRange::required_capacity(payload_len: usize) -> usize` in `kamish-outbox` to calculate the exact reservation size required for an `OutboxRange`.
- Updated `BrokerServer::produce_at_epoch` to reserve `required_range_size` rather than an estimate before writing to disk.
- Since `required_range_size == range.byte_size()`, `reservation.commit(range)` never requires additional bytes at commit time. Commitment is mathematically guaranteed to be infallible (`range.byte_size() <= reservation.reserved_bytes()`).

### Pillar 2: Infallibility Guard Helper
- Introduced `OutboxReservation::is_infallible(&self, range: &OutboxRange) -> bool` confirming that when `range.byte_size() <= self.reserved_bytes`, commitment cannot fail due to capacity limits.

### Pillar 3: Graceful Post-Append Saturation Handling (WAL-Backed Recovery)
- In `BrokerServer::produce_at_epoch`, if an adversarial test or extreme memory condition forces commit growth rejection on an admitted reservation, the broker:
  1. Catches the error and logs a structured warning with topic, partition, offset, and target peer.
  2. Continues iterating over any remaining destination reservations (preventing partial pipeline abandonment).
  3. Relies on the fact that `PartitionLog` already persisted `RecordContext` containing `replication_targets`.
  4. Returns `Ok(ProduceResponse)` because the local record is durable and the replication watermark (`durable_partition_watermark`) remains at or below `offset`.
  5. The obligation is autonomously refilled and staged into `outbox` via `BrokerServer::refill_outbox` / `repopulate_outbox_from_log` as soon as the outbox drains or upon broker restart.

---

## 4. Files Changed

| File Path | Changes | Rationale |
| :--- | :--- | :--- |
| [`crates/kamish-outbox/src/lib.rs`](file:///E:/Kamish/crates/kamish-outbox/src/lib.rs) | Added `OutboxRange::required_capacity`, added `OutboxReservation::is_infallible` | Enables exact pre-allocation and explicit infallibility checking. |
| [`cmd/kamish-broker/src/lib.rs`](file:///E:/Kamish/cmd/kamish-broker/src/lib.rs) | Updated `produce_at_epoch` to reserve `required_range_size`; made post-append commit loop fault-resilient with WAL retention | Guarantees infallible commitment on produce path and prevents dropped replication obligations. |
| [`cmd/kamish-broker/tests/outbox_commit_resilience.rs`](file:///E:/Kamish/cmd/kamish-broker/tests/outbox_commit_resilience.rs) | **New test suite** (7 comprehensive integration tests) | Validates all 7 required resilience and recovery properties. |

---

## 5. Regression Tests Added

All 7 test scenarios specified in the objective were implemented in [`cmd/kamish-broker/tests/outbox_commit_resilience.rs`](file:///E:/Kamish/cmd/kamish-broker/tests/outbox_commit_resilience.rs):

1. **`test_record_size_exceeding_estimate_retains_durable_wal_obligation`:**  
   Proves that when a reservation estimate is smaller than actual range size and outbox is saturated at commit time, the durable WAL record is preserved and autonomously recovered via `repopulate_outbox_from_log` once capacity frees up.
2. **`test_exact_preallocation_guarantees_infallible_commit`:**  
   Proves that pre-allocating via `OutboxRange::required_capacity` makes `is_infallible` true and commitment succeed without error.
3. **`test_capacity_reservation_fails_before_wal_append`:**  
   Proves that when outbox capacity is exhausted, admission control rejects the write **before** touching disk, keeping the WAL high watermark unchanged and session state clean.
4. **`test_reservation_accounting_lifecycle_exhaustive`:**  
   Proves strict, leak-free byte accounting across successful commits, RAII cancellations (`Drop`), failed commit growth, and watermark advancement.
5. **`test_restart_recovery_of_unreplicated_obligations`:**  
   Proves that when a broker abruptly terminates with un-replicated records in the WAL, restarting the broker on the same data directory automatically reconstructs all pending obligations into the outbox.
6. **`test_concurrent_producers_competing_under_bounded_capacity`:**  
   Proves that 12 barrier-synchronized concurrent producers competing for an outbox that fits only 3 records results in exactly 3 admissions, 9 backpressure rejections, and zero byte-accounting drift.
7. **`test_multi_destination_partial_saturation_atomic_rollback`:**  
   Proves all-or-nothing atomic admission across multi-destination replication pipelines: if destination B is full, destination A's reservation is rolled back, and zero WAL writes occur.

---

## 6. Verification Commands & Actual Outcomes

| Command | Exit Code | Output Summary |
| :--- | :---: | :--- |
| `cargo fmt --all -- --check` | `0` | Formatted cleanly; 0 diffs. |
| `cargo test -p kamish-broker --test outbox_commit_resilience` | `0` | **7 passed; 0 failed; 0 ignored** |
| `cargo test --workspace -j 2` | `0` | **95 passed; 0 failed; 0 ignored** across all 14 workspace crates |
| `cargo clippy --workspace --all-targets -j 2` | `0` | **0 errors; 0 warnings** |
| `cargo run -p kamish-ctl -- demo` | `0` | **All 4 demonstration scenarios passed deterministically** |

---

## 7. Remaining Limitations and Unresolved Questions

1. **Large WAL Backlog Memory Footprint on Cold Restart:**  
   If a broker restarts after days of network partition with hundreds of thousands of unreplicated records, `BrokerServer::new` will repopulate the outbox up to its configured `max_bytes` bound. Records exceeding `max_bytes` remain safely on disk in the WAL. While correct, streaming chunked pagination will be beneficial for large partition logs.
2. **Adjacent Issue Recorded (Scope Boundary Preserved):**  
   As observed in the Milestone 5 audit, `ReplicationAgent` handles steady-state `OffsetGap` errors via backoff and drops ranges after 3 retries rather than immediately initiating a WAL backfill stream. Per instructions, this adjacent issue was left untouched within scope boundaries and recorded for separate remediation.

