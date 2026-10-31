# Milestone 5.2: Reproduction Plan & Deterministic Verification

This document specifies the exact environment, CLI commands, and test cases required to reproduce the Milestone 5.2 audit findings independently.

---

## 1. System Baseline Requirements

- **OS:** Windows 10/11 or Linux (POSIX compliant).
- **Rust Toolchain:** `stable` or `nightly` (tested on `1.83.0`).
- **Target Repository:** `E:\Kamish` (branch `main`).
- **Dependencies:** MSVC C++ Build Tools (on Windows) or `build-essential` (on Linux).

---

## 2. Global Test Execution Commands

To execute the entire test suite and quality gates:

```powershell
# 1. Check workspace formatting
cargo fmt --all -- --check

# 2. Run the full workspace test suite (127 tests across 42 suites)
cargo test --workspace -j 2

# 3. Verify zero Clippy warnings across all targets
cargo clippy --workspace --all-targets -j 2
```

---

## 3. Deterministic Reproduction by Audit Area

### Scenario 1: Three-Region Topology, Link Degradation & Independent Progress
- **Target Invariants:** R1, R8 (Capacity bound, multi-destination queue isolation).
- **Test File:** `cmd/kamish-broker/tests/three_region_convergence_audit.rs`
- **Execution Command:**
  ```powershell
  cargo test -p kamish-broker --test three_region_convergence_audit test_three_region_saturation_and_independent_progress -- --exact --nocapture
  ```
- **Observed Behavior:**
  - Broker A produces to US_EAST and AP_SOUTH.
  - US_EAST outbox saturates at 200 bytes.
  - Third produce to `global-events` returns `sovereignty_allowed: false` with `denial_reason: "Backpressure: destination outbox for 'US_EAST' full"`.
  - Independent stream to `asia-events` (destined only for AP_SOUTH) continues unhindered.

---

### Scenario 2: Broker Restart Recovery with Unreplicated Backlog
- **Target Invariants:** R9, R10 (Restart repopulation, watermark advancement).
- **Test File:** `cmd/kamish-broker/tests/three_region_convergence_audit.rs`
- **Execution Command:**
  ```powershell
  cargo test -p kamish-broker --test three_region_convergence_audit test_broker_restart_recovery_and_watermark_advancement -- --exact --nocapture
  ```
- **Observed Behavior:**
  - Broker A crashes with 3 un-replicated records on disk.
  - Restarting Broker A via `BrokerServer::try_new` automatically discovers records and repopulates the outbox.
  - Replicating to Broker B advances watermark monotonically from 0 to 3.
  - Outbox drains to 0 bytes.

---

### Scenario 3: Non-Contiguous Gap Detection & Idempotent Deduplication
- **Target Invariants:** R3, R4, R6, R7 (Gap safety, deduplication, conflict rejection).
- **Test File:** `cmd/kamish-broker/tests/three_region_convergence_audit.rs`
- **Execution Commands:**
  ```powershell
  cargo test -p kamish-broker --test three_region_convergence_audit test_receiver_offset_gap_detection_and_watermark_preservation -- --exact --nocapture
  cargo test -p kamish-broker --test three_region_convergence_audit test_receiver_idempotent_deduplication_and_conflict_rejection -- --exact --nocapture
  ```
- **Observed Behavior:**
  - Out-of-order offset 2 sent before offset 1 is rejected with `offset_gap: expected 1, received 2`.
  - Watermark remains at 1; receiver log does not skip or tear.
  - Re-sending offset 0 with identical payload returns `idempotent_duplicate_noop`.
  - Re-sending offset 0 with conflicting payload returns `replication_history_conflict`.

---

### Scenario 4: Dynamic Policy Revocation Blocking Backlog Recovery
- **Target Invariant:** R11 (Policy enforcement across recovery).
- **Test File:** `cmd/kamish-broker/tests/three_region_convergence_audit.rs`
- **Execution Command:**
  ```powershell
  cargo test -p kamish-broker --test three_region_convergence_audit test_policy_revocation_blocks_outbox_recovery_repopulation -- --exact --nocapture
  ```
- **Observed Behavior:**
  - Record produced when US_EAST was legal.
  - Policy updated to revoke US_EAST.
  - Recovery repopulation halts for US_EAST, returning 0 records staged and logging policy refusal.

---

### Scenario 5: Disk Sync Failure and Writer Fencing
- **Target Invariants:** R12, R13 (Append durability consistency and epoch fencing).
- **Test File:** `crates/kamish-log/tests/append_io_failures.rs`
- **Execution Command:**
  ```powershell
  cargo test -p kamish-log --test append_io_failures -- --nocapture
  ```
- **Observed Behavior:**
  - Injected `sync_all` failure marks writer as fenced (`LogError::WriterFenced`).
  - Subsequent appends immediately fail with `WriterFenced`.
  - On restart, un-synced partial data is truncated and clean log state is recovered.

