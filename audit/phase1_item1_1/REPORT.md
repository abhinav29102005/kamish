# Phase 1, Item 1.1 Verification Audit Report

**Date:** 2026-11-01  
**Commit:** `Phase 1 Item 1.1 Corrections`  
**Purpose:** Record complete, unedited execution evidence of all workspace verification checks for Item 1.1 refactoring.

## Baseline Execution Evidence Inventory

| Target Verification Command | Log Artifact File | Result Status |
| :--- | :--- | :--- |
| `cargo check --workspace --all-targets` | [cargo_check.txt](cargo_check.txt) | **Exit Code 0** (0 errors) |
| `cargo build --workspace` | [cargo_build.txt](cargo_build.txt) | **Exit Code 0** (clean build) |
| `cargo fmt --all -- --check` | [cargo_fmt.txt](cargo_fmt.txt) | **Exit Code 0** (0 diffs) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | [cargo_clippy.txt](cargo_clippy.txt) | **Exit Code 0** (0 warnings) |
| `cargo test --workspace` | [cargo_test_workspace.txt](cargo_test_workspace.txt) | **Exit Code 0** (108 passed, 0 failed) |
| `cargo test --workspace --all-features` | [cargo_test_workspace_all_features.txt](cargo_test_workspace_all_features.txt) | **Exit Code 0** (108 passed, 0 failed) |
| `cargo test --manifest-path early_research/probes/Cargo.toml --lib` | [probes_lib.txt](probes_lib.txt) | **Exit Code 0** (11 passed, 0 failed) |
| `cargo test --manifest-path early_research/probes/Cargo.toml --test remaining_gaps` | [probes_remaining_gaps.txt](probes_remaining_gaps.txt) | **Exit Code 101** (4 passed, 2 failed as documented in Phase 0 contract) |
| `npm --prefix frontend test` | [frontend_test.txt](frontend_test.txt) | **Exit Code 0** (4 passed, 0 failed) |
| `npm --prefix frontend run build` | [frontend_build.txt](frontend_build.txt) | **Exit Code 0** (production bundle built) |
| `npm --prefix packages/kamish-client test` | [sdk_test.txt](sdk_test.txt) | **Exit Code 0** (23 passed, 0 failed) |
| `npm --prefix packages/kamish-client run test:integration` | [sdk_integration.txt](sdk_integration.txt) | **Exit Code 0** (1 passed, 0 failed against real Rust broker) |
| `npm --prefix packages/kamish-client run test:package` | [sdk_package.txt](sdk_package.txt) | **Exit Code 0** (package tarball verified) |

## Item 1.1 Architectural Corrections Summary

1. **Separation of Consensus vs Local Replication State (ADR-0017):**
   - `MetadataAuthority` now contains *only* consensus-backed state: epoch CAS (`compare_and_swap_epoch`), leases (`grant_lease`, `renew_lease`, `revoke_lease`), policy floor (`get_policy_floor`, `advance_policy_floor`), source generation (`issue_source_identity`), audit checkpoints (`commit_audit_checkpoint`, `read_audit_checkpoint`), deployment mode (`deployment_mode`), and ownership lookup (`get_ownership`, `set_ownership`).
   - `LocalReplicationTracker` was extracted for node-local replication tracking (`update_watermark`, `get_watermark`, `record_outbox_stream`, `get_outbox_stream`). Steady-state replication progress is completely decoupled from metadata consensus overhead.

2. **Local Monotonic Lease Clock Rules (docs/CONTRACT.md §2.3.1):**
   - Implemented `MonotonicClock`, `SystemMonotonicClock`, `MockMonotonicClock`, and `LocalLease`.
   - Lease validity is computed strictly against a local monotonic clock (`Instant`) measured from grant receipt minus safety allowances (`max_clock_drift` + `transit_delay`), never remote wall-clocks.
   - Verified via `test_lease_monotonic_clock_pause_skew_and_expiry`.

3. **Broker MultiRegion Fail-Closed Test:**
   - Added `cmd/kamish-broker/tests/multiregion_ownership.rs` testing that unregistered streams in `MultiRegion` mode fail closed, while registered owners succeed.
   - Formally documented that fail-closed admission is an edge gate and does not constitute distributed authority.


## Requirements, Invariants, and Scope Matrix (Re-derived from project.md & worktillnow.md §23)

### Functional Requirements (`project.md`)
| ID | Requirement | Current Status | Evidence / Notes |
| :--- | :--- | :--- | :--- |
| **FR-01** | Create topic bound to class | **Partial** | Metadata helper and first-local-record partition contract; no authenticated lifecycle RPC. |
| **FR-02** | Produce batches with idempotent producer sessions | **Partial** | Local single-record dedupe tested; true batch protocol and regional session recovery missing. |
| **FR-03** | Fetch records in offset order up to committed HWM | **Tested** | Local HWM verified; durable consumer-group semantics missing. |
| **FR-04** | One signed decision at every movement point | **Partial** | Core produce/replication enforcement; unified admin/gateway path still needed. |
| **FR-05** | Mirror replication with offsets preserved | **Tested (Narrow)** | Single-record offset/payload verified; complete-envelope/prefix proof pending. |
| **FR-06** | Bounded durable outbox and watermark resume | **Partial** | In-memory accounting + source-log replay; durable cursor/intent disk budget incomplete. |
| **FR-07** | Owner/epoch fencing | **Partial** | Local registered-owner and MultiRegion fail-closed rejection tested; etcd consensus pending (Item 1.2). |
| **FR-08** | Policy-eligible promotion | **Partial** | Local helper gate; replica proof and global promotion protocol missing. |
| **FR-09** | Per-class degradation | **Partial** | Enum/concepts exist; complete distinct per-class budgeting and blocking/degradation behavior not implemented. |
| **FR-10** | Canary scanner and invariant checkers | **Partial** | Invariant checkers and probe suites exist; background continuous scanner pending. |
| **FR-11** | Sealed-only (ciphertext) destination mode | **Contained** | Explicitly rejected/unsupported until secure implementation. |
| **FR-12** | Ciphertext relay transit | **Partial** | Planner allowlist tests; real relay transport/ciphertext proof missing. |
| **FR-13** | Hash-chained, region-local decision log | **Partial** | Local format-2 implementation; completeness anchoring/writer ownership incomplete. |
| **FR-14** | Derive mode with offset lineage | **Not implemented** | Deferred per project specification. |

### Non-Functional Requirements (`project.md`)
| ID | Requirement | Current Status | Evidence / Notes |
| :--- | :--- | :--- | :--- |
| **NFR-01** | Control plane off write path; pinned continuity | **Design in place, not yet tested** | Trait refactored (ADR-0017): steady-state write path uses `LocalReplicationTracker` without consulting quorum. Outage scenario test pending Item 1.2 multi-broker setup. |
| **NFR-02** | Fail-closed on invalid/stale/replayed policy | **Tested (Local)** | Local verification and signature checks covered; durable restart floor and deployed policy service missing. |
| **NFR-03** | Bounded memory/disk & class pressure | **Partial** | Queue caps exist; full disk/lifecycle fairness incomplete. |
| **NFR-04** | Smoothed, windowed route evidence | **Partial** | Policy filtering, current metrics and dwell/hysteresis exist; smoothing/window/breaker proof incomplete. |
| **NFR-05** | Every number links to run/config/date | **Improved** | Raw, unedited execution logs captured in `audit/phase1_item1_1/`; calibration and runs not done. |
| **NFR-06** | All inter-component traffic uses mTLS 1.3 | **Not implemented** | Plaintext loopback; mTLS scheduled for Phase 2 / W09. |

### Core Invariants (`project.md`)
| ID | Invariant | Current Status | Evidence / Notes |
| :--- | :--- | :--- | :--- |
| **INV-1** | Dense monotonic partition offsets | **Tested (Local)** | Local append/replay tests pass. |
| **INV-2** | ACKed record on majority of local replicas | **Not implemented** | Single-file sync only; replication across 3 replicas scheduled for Phase 3. |
| **INV-3** | New leader holds all ACKed records | **Not established** | Requires replica/quorum/promotion history (Phase 3). |
| **INV-4** | Fetch never returns records beyond committed HWM | **Tested (Local)** | Local HWM verified in broker and client tests. |
| **INV-5** | Destination log is exact prefix of source log | **Tested (Narrow)** | Single-record offset/payload verified; complete-envelope limitations remain. |
| **INV-6** | Retrying an applied range is a no-op | **Tested (Local)** | Local single-record deduplication verified; regional session history pending. |
| **INV-7** | No two regions commit writes for same partition/epoch | **Open Contract Gap** | Probe `independent_regions_must_not_both_accept_unregistered_epoch_one_writes` correctly failing until etcd authority in Item 1.2. |
| **INV-8** | No payload outside legal set (Residency) | **Tested (Selected)** | Selected class/mode checks and synthetic scans; universal verification pending. |
| **INV-9** | Invalid, unsigned or rolled-back policy never applied | **Tested (Local)** | Local signature/cache checks pass; durable cluster floor pending. |

### Open Work Packages
| ID | Description | Current Status |
| :--- | :--- | :--- |
| **W15** | Log segment/index/cache and bounded recovery | **Open** | No segment recovery claims; log uses single-file format with no segment abstraction yet. Bounded recovery remains an open item. |
