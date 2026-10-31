# Phase 1, Item 1.2: Authoritative Multi-Region Metadata (etcd 3.7.3) Audit Report

Date: 2026-11-01
Status: VERIFIED & COMPLETE

---

## 1. Scope & Objective

Implement production-grade authoritative multi-region metadata consensus using `etcd 3.7.3` and pure-Rust TLS (`etcd-client` crate with `tls-ring`), enforcing:
- Independent, monotonically advancing epoch keys (`kamish/streams/{topic}/{partition}/epoch`) that are never lease-attached.
- Atomic single-Txn CAS for partition ownership (`kamish/streams/{topic}/{partition}/owner`).
- Local monotonic clock lease validation with drift and transit allowance (`max_clock_drift_ms = 200`, `transit_delay_allowance_ms = 100`).
- Idempotent ambiguous commit retry handling.
- Quorum loss fail-closed semantics (writes stop when < 2/3 members alive).
- Cluster cold restart persistence of epoch fences.
- Plain `cargo test --workspace` skips without failure when etcd is not configured; `./scripts/run_etcd_tests.sh` (or `KAMISH_ETCD_TESTS=1`) runs the 8 full integration tests against a live 3-member cluster.

---

## 2. etcd-client Crate & etcd 3.7.3 Verification

### 2.1 etcd-client Crate Analysis
- **Crate Version:** `etcd-client = 0.21.1` (exact pin)
- **Features Enabled:** `default-features = false`, `features = ["tls-ring"]`
- **TLS Engine:** Pure-Rust TLS backed by `rustls 0.23` and `ring 0.17`. Zero OpenSSL runtime dependencies.
- **Protobuf / gRPC Engine:** `tonic 0.14.9`, `prost 0.14.4`.
- **Protoc Requirement:** `etcd-client`'s `build.rs` compiles `.proto` definitions at build time. Solved hermetically by downloading official Google `protoc 29.3` binary and standard includes into `bin/protoc` and `bin/include`, configured via `.cargo/config.toml` (`PROTOC` and `PROTOC_INCLUDE`).
- **Compatibility Verification:** Tested against official etcd v3.7.3 using gRPC over TLS. All RPCs (`Txn`, `LeaseGrant`, `LeaseKeepAlive`, `LeaseRevoke`, `Put`, `Get`) work against etcd 3.7.3.

### 2.2 etcd v3.7.3 Binary Verification
- **Official Release:** `https://github.com/etcd-io/etcd/releases/download/v3.7.3/etcd-v3.7.3-linux-amd64.tar.gz`
- **Computed SHA-256:** `5dfa9cc3b3ce3b4b491fbcc2057d3daa166bcc6676f743fcd20f8e5760cb9775`
- **Installed Paths:** `bin/etcd`, `bin/etcdctl` (gitignored).

### 2.3 mTLS Certificate Generation
- Generated via `scripts/generate_test_certs.sh`:
  - Self-signed Test CA (`certs/ca.crt`, `certs/ca.key`).
  - 3 peer/member certificates with SANs (`us-east`, `eu-central`, `ap-south`) covering `127.0.0.1`, `localhost`, and internal DNS names.
  - Client certificate (`certs/client.crt`, `certs/client.key`) with clientAuth usage.
  - Permissions set to `0600`. Certificates directory is strictly gitignored (`certs/`). Zero secrets committed.

---

## 3. Implementation Details

### 3.1 `EtcdMetadataAuthority` (`crates/kamish-meta/src/etcd.rs`)
- Implements `MetadataAuthority` trait.
- `compare_and_swap_epoch`:
  - Acquires lease for `lease_duration_ms` via `client.lease_client().grant()`.
  - Executes atomic etcd Txn:
    - `When`: `epoch_key == expected_epoch` (or `version == 0` for initial epoch).
    - `Then`: `put(owner_key, OwnerRecord, lease=lease_id)`, `put(epoch_key, new_epoch, lease=None)`.
    - `Else`: `get(epoch_key)`.
  - If CAS fails:
    - Checks for ambiguous commit replay (if `actual_epoch == new_epoch` and owner is current claimant, returns success idempotently).
    - If another claimant won: immediately revokes the unused lease and returns `MetaError::StaleEpoch`.
  - If transport/connection error occurs (e.g. during leader failover):
    - Retries up to 4 times with backoff, resetting the internal connection channel and checking consensus ownership before re-proposing.
  - Registers `LocalLease` with monotonic receipt timestamp and validates partition writes against `clock.now_monotonic_ms() < monotonic_expiry_ms`.

### 3.2 Monotonic Clock Lease Validation
- Broker local admission uses `LocalLease::is_valid(clock.now_monotonic_ms())`.
- Formula: `valid_duration = server_duration - max_drift (200ms) - transit_allowance (100ms)`.
- If client process pauses or thread hangs longer than `valid_duration`, local admission immediately stops and returns `None`, guaranteeing broker fail-closed behavior before etcd server-side lease expiry.

---

## 4. Integration Test Results

Test target: `crates/kamish-meta/tests/etcd_authority.rs` (orchestrated by `./scripts/run_etcd_tests.sh`)
Cluster topology: 3 members (`us-east`: 2379/2380, `eu-central`: 2381/2382, `ap-south`: 2383/2384) with mTLS.

| Test Name | Scenario Tested | Result |
| :--- | :--- | :--- |
| `test_01_competing_claimants_race_for_epoch_1_repeated` | 10 repeated concurrency races between region-a and region-b. Exactly one claimant wins epoch 1 in every round; loser gets `StaleEpoch`. | PASS |
| `test_02_epoch_never_regresses_after_owner_key_expiry` | Epoch 1 established with 2s lease. After server lease expiry, owner key expires, but persistent epoch key remains 1. Stale proposal at 0 rejected. CAS 1->2 succeeds. | PASS |
| `test_03_lease_ttl_expiry_under_paused_client_and_monotonic_clock` | 5s lease granted. Mock monotonic clock advances 4800ms. Local lease fails closed immediately while server lease still has 200ms. | PASS |
| `test_04_keepalive_lost_broker_stops_acking` | 3s lease granted. Simulates lost keepalive by advancing local clock past lease limit. Broker ceases ACKing immediately. | PASS |
| `test_05_policy_floor_and_source_generation` | Policy floor monotonicity (0 -> 10, regression to 9 rejected). Source identity generation strictly monotonic across nodes (gen 1, gen 2). | PASS |
| `test_06_ambiguous_commit_outcome_handling_and_retry_idempotence` | Proposes 1 -> 2. Network drop simulated. Retry of proposal 1 -> 2 by same claimant succeeds idempotently without error. Replay by competing claimant rejected. | PASS |
| `test_07_quorum_loss_fail_closed_and_member_restart` | Kill member 3 (`ap-south`): 2/3 quorum holds, CAS succeeds. Kill member 2 (`eu-central`): 1/3 members alive, quorum lost, writes fail closed. Restart members 2 and 3: cluster heals, write succeeds at epoch 3. | PASS |
| `test_08_cold_restart_epoch_and_fence_persistence` | Epoch 42 committed. Whole cluster stopped cleanly (`stop_etcd_cluster.sh`). Whole cluster restarted (`start_etcd_cluster.sh`). Stale CAS rejected; bump 42 -> 43 succeeds. | PASS |

Unedited test execution log saved to: `audit/phase1_item1_2/etcd_test_output.txt`.

---

## 5. Non-Regression & Workspace Test Status

- `cargo test --workspace`: 108 passed, 0 failed, 8 skipped (etcd tests skipped gracefully when `KAMISH_ETCD_TESTS` unset).
- `cargo test --workspace --all-features`: 108 passed, 0 failed, 8 skipped.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings.
- `cargo fmt --check`: Clean.

