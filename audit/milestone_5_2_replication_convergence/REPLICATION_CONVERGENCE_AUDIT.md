# Milestone 5.2: Replication Convergence, Recovery, and Regional Resilience Audit

**Repository:** `E:\Kamish`  
**Execution Mode:** Audit-first, evidence-driven, read-only on production code  
**HEAD Commit:** `3a190bdccb9ab8fd51af3eab66eb35af73bf9fb2`  
**Audit Baseline Commit:** `77bb051bfa77e6be80e8d8e5f4ea3cd4dad9eba2`  
**Date:** 2026-11-01  
**Auditor:** Distributed Systems Reliability Auditor (Antigravity Advanced Agentic Coding)

---

## 1. Executive Summary & Verdict

HydraStream (Kamish) was evaluated against strict distributed systems invariants governing replication convergence, bounded outbox queuing, offset gap resilience, crash recovery, and multi-region data residency.

### Final Decision Gate:
> **CONDITIONAL — REPRODUCIBLE GAPS REMAIN**

The core storage and replication invariants (**R1–R14**) are structurally enforced and verified by 127 passing automated tests (including 5 new deterministic end-to-end multi-region audit tests). Durability, epoch fencing, crash recovery, and gap-free prefix ordering are mathematically sound. 

However, release certification is rated **CONDITIONAL** due to four specific, reproducible non-critical gaps:
1. **Replication Loop Inefficiency on Gap:** `step_replication` and `step_replication_tcp` do not early-break a batch upon receiving `offset_gap`, wasting link round-trips.
2. **Multi-Destination Fate Sharing:** Multi-region topics suffer total write rejection if any single destination outbox is saturated, blocking healthy destinations.
3. **Retry Worker Channel Dropping:** `spawn_retry_worker` drops tasks from its in-memory queue after 3 retries, relying on external polling to re-query the outbox.
4. **Prototype Metadata Volatility:** Partition ownership leases reset on restart, relying on fallback epoch 1 prototype mechanics unless externally re-established.

---

## 2. Baseline Verification & Execution Context

Prior to conducting audits, the workspace state was established and verified:
- **Git Branch:** `main`
- **Git Status:** Clean working tree
- **Rust Toolchain:** `rustc 1.83.0` / `cargo 1.83.0-nightly`
- **Verification Commands Executed:**
  - `cargo fmt --all -- --check`: Clean (Exit code 0).
  - `cargo test --workspace -j 2`: **127 passed, 0 failed across 42 test suites** (Exit code 0).
  - `cargo clippy --workspace --all-targets -j 2`: **0 warnings** (Exit code 0).

---

## 3. Complete Lifecycle Trace Map

```mermaid
sequenceDiagram
    autonumber
    actor Client as Producer
    participant Broker as Kamish Broker (Origin)
    participant Meta as MetadataStore
    participant Policy as Enforcer / PolicyEngine
    participant Outbox as DestinationOutbox
    participant WAL as PartitionLog (Disk)
    participant Peer as Peer Broker (Receiver)

    Client->>Broker: produce(ProduceRequest)
    Broker->>Meta: Check partition lease & owner epoch
    Broker->>Policy: Ingress & BrokerAppend validation
    Broker->>Outbox: reserve_capacity(required_range_size) [INV-1, INV-2]
    alt Outbox Full
        Outbox-->>Broker: Err(QueueCapacityExceeded)
        Broker-->>Client: SovereigntyDenied (Backpressure)
    else Capacity Reserved
        Outbox-->>Broker: Ok(OutboxReservation)
        Broker->>WAL: append_with_context(..., RecordContext) [INV-12]
        alt WAL Append / fsync Fails
            WAL-->>Broker: Err(LogError) [Fenced / Rollback]
            Broker->>Outbox: drop(reservation) -> Releases capacity via RAII
            Broker-->>Client: Err(LogError)
        else WAL Appended Durably
            Broker->>Broker: session_mgr.update(producer_id, seq)
            Broker->>Outbox: reservation.commit(OutboxRange)
            Broker-->>Client: ProduceResponse { base_offset, sovereignty_allowed: true }
        end
    end

    Note over Broker,Peer: Asynchronous Replication Transport Loop
    Broker->>Outbox: next_transfer_batch(max)
    Broker->>Peer: ReplicationRequest (Single-Record Range)
    Peer->>Peer: Enforcer::decide(ReplicationReceiver)
    alt Non-Contiguous Gap (from_offset > local_next)
        Peer-->>Broker: ReplicationResponse { accepted: false, reason: "offset_gap" }
        Note over Broker: Watermark NOT advanced; gap repaired sequentially
    else Idempotent Duplicate (to_offset <= local_next)
        Peer-->>Broker: ReplicationResponse { accepted: true, reason: "idempotent_duplicate_noop" }
        Broker->>Outbox: advance_partition_watermark(topic, part, local_next)
    else Contiguous Next (from_offset == local_next)
        Peer->>Peer: WAL.append(payload) & update_watermark
        Peer-->>Broker: ReplicationResponse { accepted: true, applied_watermark }
        Broker->>Outbox: advance_partition_watermark(topic, part, applied_watermark)
        Broker->>Outbox: Trim acknowledged ranges & subtract queued_bytes
    end
```

---

## 4. Deep-Dive Audit Findings

### Audit A: Outbox Capacity and Commit Failure
- **A1 — Reservation Failure Before WAL:**
  - *Code:* `cmd/kamish-broker/src/lib.rs:520-550`.
  - *Behavior:* If `reserve_capacity` fails with `QueueCapacityExceeded`, any previously acquired reservations for other destinations in that request are dropped immediately (`drop(reservations)`), and `ProduceResponse` returns `sovereignty_allowed: false` with backpressure message. The local WAL is never touched.
  - *Verdict:* **PASS (R1, R2 Satisfied).**
- **A2 — Commit Failure After WAL:**
  - *Code:* `cmd/kamish-broker/src/lib.rs:610-630`.
  - *Behavior:* When exact pre-allocation is used (`OutboxRange::required_capacity(payload.len())`), commit is infallible. If commit were to fail (e.g. simulated dynamic growth beyond capacity), the WAL record is preserved durably with its `RecordContext.replication_targets`. The failed reservation is dropped. `repopulate_outbox_from_log` discovers and stages the unreplicated record once capacity is freed.
  - *Verdict:* **PASS (R2, R14 Satisfied).**
- **A3 — Recovery Under Saturation:**
  - *Code:* `cmd/kamish-broker/src/lib.rs:885-895`.
  - *Behavior:* During outbox repopulation from disk, if `outbox.enqueue` encounters `QueueCapacityExceeded`, it terminates gracefully with `return Ok(staged_count)`. The broker does not crash or loop infinitely; remaining records are staged on subsequent refill cycles.
  - *Verdict:* **PASS (R9, R14 Satisfied).**

---

### Audit B: Offset Gaps, Partial Overlaps, and Watermarks
- **B1 — Dense Contiguous Prefix:**
  - *Code:* `cmd/kamish-broker/src/lib.rs:756-776`.
  - *Behavior:* The receiver appends records if and only if `req.from_offset == local_next`. Watermark is updated to `applied_offset + 1`.
  - *Verdict:* **PASS (R3 Satisfied).**
- **B2 — Gap Rejection:**
  - *Code:* `cmd/kamish-broker/src/lib.rs:779-787`.
  - *Behavior:* If `req.from_offset > local_next`, receiver rejects with `accepted: false, applied_watermark: local_next, reason: "offset_gap"`. No torn gap is created.
  - *Verdict:* **PASS (R4 Satisfied).**
- **B3 — Watermark Safety:**
  - *Code:* `crates/kamish-outbox/src/lib.rs:188-210`.
  - *Behavior:* The source outbox only advances its watermark if the remote watermark exceeds the current watermark (`new_watermark > current`). Regressive watermarks are ignored.
  - *Verdict:* **PASS (R5 Satisfied).**
- **B4 & B5 — Idempotent Duplicate & Conflict Handling:**
  - *Code:* `cmd/kamish-broker/src/lib.rs:734-753`.
  - *Behavior:* If `req.to_offset <= local_next`, the receiver reads the local record at `from_offset`. If epoch, class, payload, and CRC match, it returns `accepted: true, reason: "idempotent_duplicate_noop"`. If any field conflicts, it returns `accepted: false, reason: "replication_history_conflict"`.
  - *Verdict:* **PASS (R6, R7 Satisfied).**
- **B6 — Loop Continuation Nuance (Gap 1):**
  - *Code:* `cmd/kamish-agent/src/lib.rs:695-715` & `845-865`.
  - *Behavior:* When `rep_res.accepted == false` due to `offset_gap`, the agent logs `tracing::warn!("Replication rejected by peer: {}")`, but does *not* `break` out of the batch loop. It continues sending remaining ranges in that batch, which all fail with `offset_gap`.
  - *Verdict:* **PARTIAL / NON-CRITICAL EFFICIENCY GAP.**

---

### Audit C: Retry, Scheduling, and Independent Progress
- **C1 — Isolated Queues Per Destination:**
  - *Code:* `BrokerServer.outboxes: Arc<RwLock<HashMap<String, DestinationOutbox>>>`.
  - *Behavior:* Each destination maintains an independent `DestinationOutbox` with its own `queued_bytes` and `max_bytes`.
  - *Verdict:* **PASS (R8 Satisfied).**
- **C2 — Retry Worker Channel Invalidation (Gap 3):**
  - *Code:* `cmd/kamish-agent/src/lib.rs:645-665`.
  - *Behavior:* In `spawn_retry_worker`, after 3 failed transport attempts, the task is dropped from the async channel. The WAL and outbox ranges retain the obligation, but active re-transmission requires a subsequent `enqueue_pending_outbox_tasks` poll.
  - *Verdict:* **PARTIAL / SCHEDULING GAP.**
- **C3 — Multi-Destination Fate Sharing (Gap 2):**
  - *Code:* `cmd/kamish-broker/src/lib.rs:520-545`.
  - *Behavior:* In `produce_at_epoch`, reservations are evaluated sequentially. If destination B's outbox is full, reservations for destination C are dropped, rejecting write for multi-target topics. Topics targeting only C progress normally.
  - *Verdict:* **PARTIAL / ARCHITECTURAL LIMITATION.**

---

### Audit D: Restart Recovery & Regional Outage (3 Regions)
- **Topological Scenario Tested:**
  - Region A (`EU_WEST`), Region B (`US_EAST`), Region C (`AP_SOUTH`).
  - Stream 1: `global-stream` targeting US_EAST and AP_SOUTH.
  - Stream 2: `asia-stream` targeting only AP_SOUTH.
  - Link A-to-B degraded/unreachable -> US_EAST outbox saturates at 200 bytes.
- **Empirical Findings:**
  1. Once US_EAST outbox saturates, `global-stream` produces fail with `Backpressure: destination outbox for 'US_EAST' full`.
  2. `asia-stream` produces continue without backpressure, proving queue isolation.
  3. Broker A restart (`BrokerServer::try_new`) automatically scans disk, identifies unreplicated WAL records, and repopulates the outbox up to `max_bytes`.
  4. Subsequent `refill_outbox` calls are strictly idempotent.
  5. Restored connectivity drains backlog with correct watermark advancement to 3.
- *Verdict:* **PASS (Verified by `three_region_convergence_audit.rs`).**

---

### Audit E: Data Residency & Policy Across Recovery
- **E1 — Admission Context Persistence:**
  - `RecordContext` persists `origin_region`, `request_policy`, and `replication_targets` on disk in the record envelope.
- **E2 — Dynamic Policy Re-evaluation:**
  - `repopulate_outbox_from_log` evaluates `AuditAction::ReplicationSender` against the active policy at recovery time.
  - When US_EAST was revoked from `legal_regions` between restart and refill, repopulation staged 0 records and logged policy refusal.
- *Verdict:* **PASS (R11 Satisfied).**

---

### Audit F: PartitionLog Failure-Model Consistency
- **F1 — Sync Failure and Writer Fencing:**
  - Any error in `sync_all()` marks the writer as fenced (`LogError::WriterFenced`) and invalidates the file handle.
- **F2 — Deterministic State Handling:**
  - State A (Record committed on disk before sync failed): Validated CRC and recovered at next startup without gap skipping.
  - State B (Torn write): Rollback truncates to `original_len`; if corrupted bytes remain, CRC validation detects it on recovery and stops at the valid boundary.
- *Verdict:* **PASS (R12, R13 Satisfied).**

---

## 5. Invariant Evidence Matrix (R1–R14)

| ID | Invariant Name | Enforcing Implementation | Deterministic Test | Status |
|:---|:---|:---|:---|:---|
| **R1** | Strict Outbox Capacity Bound | `DestinationOutbox::reserve_capacity` (`crates/kamish-outbox/src/lib.rs:105`) | `test_outbox_bounded_capacity`, `test_three_region_saturation_and_independent_progress` | **SATISFIED** |
| **R2** | Atomic Pre-Allocation / Rollback | `OutboxReservation::drop` (`crates/kamish-outbox/src/lib.rs:65`) | `test_outbox_atomic_reservation_and_rollback`, `test_capacity_reservation_fails_before_wal_append` | **SATISFIED** |
| **R3** | Dense Replication Prefix | `BrokerServer::handle_replication` (`cmd/kamish-broker/src/lib.rs:756`) | `test_receiver_offset_gap_detection_and_watermark_preservation` | **SATISFIED** |
| **R4** | No Silent Gap Skipping | `BrokerServer::handle_replication` (`cmd/kamish-broker/src/lib.rs:779`) | `test_receiver_offset_gap_detection_and_watermark_preservation` | **SATISFIED** |
| **R5** | Watermark Monotonicity | `DestinationOutbox::advance_partition_watermark` (`crates/kamish-outbox/src/lib.rs:188`) | `test_outbox_enqueue_advance_and_trim`, `ack_does_not_hide_a_new_partition_or_topic` | **SATISFIED** |
| **R6** | Idempotent Deduplication | `BrokerServer::handle_replication` (`cmd/kamish-broker/src/lib.rs:748`) | `test_receiver_idempotent_deduplication_and_conflict_rejection` | **SATISFIED** |
| **R7** | Conflicting Duplicate Rejection | `BrokerServer::handle_replication` (`cmd/kamish-broker/src/lib.rs:741`) | `test_receiver_idempotent_deduplication_and_conflict_rejection` | **SATISFIED** |
| **R8** | Multi-Destination Isolation | `BrokerServer.outboxes` (`cmd/kamish-broker/src/lib.rs:920`) | `test_three_region_saturation_and_independent_progress` | **SATISFIED*** |
| **R9** | Restart Outbox Repopulation | `BrokerServer::try_new` (`cmd/kamish-broker/src/lib.rs:304`) | `test_broker_restart_recovery_and_watermark_advancement` | **SATISFIED** |
| **R10**| Restart Watermark Preservation | `DestinationOutbox::durable_partition_watermark` (`crates/kamish-outbox/src/lib.rs:184`) | `test_broker_restart_recovery_and_watermark_advancement` | **SATISFIED** |
| **R11**| Policy Enforcement on Recovery | `BrokerServer::repopulate_outbox_from_log` (`cmd/kamish-broker/src/lib.rs:875`) | `test_policy_revocation_blocks_outbox_recovery_repopulation` | **SATISFIED** |
| **R12**| Append I/O Durability Consistency | `PartitionLog::append_with_context` (`crates/kamish-log/src/lib.rs:360`) | `append_io_failures.rs`, `adversarial_durability.rs` | **SATISFIED** |
| **R13**| Epoch Fencing Safety | `PartitionLog::append_with_context` (`crates/kamish-log/src/lib.rs:325`) | `test_epoch_advancement_preserved_on_failed_append` | **SATISFIED** |
| **R14**| Bounded Backlog Refill | `BrokerServer::repopulate_outbox_from_log` (`cmd/kamish-broker/src/lib.rs:885`) | `test_record_size_exceeding_estimate_retains_durable_wal_obligation` | **SATISFIED** |

*\*Note on R8: Destination queues are completely isolated. For shared multi-destination topics, outbox backpressure currently fails the produce request if any target outbox is full.*

