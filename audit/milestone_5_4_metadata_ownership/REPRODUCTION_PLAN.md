# KAMISH MILESTONE 5.4: REPRODUCTION PLAN
## Deterministic Verification and Edge Case Reproduction Harness

**Repository:** `E:\Kamish`  
**Git Baseline Commit:** `27cbc49fd14f32f71a8be513b0eca7f81b796ebc`

---

## 1. Quick Verification Commands

### 1.1 Non-Etcd Standard Workspace Suite (Offline & Sandboxed)
```powershell
# Format check
cargo fmt --all -- --check

# Linter quality gate
cargo clippy --workspace --all-targets -j 2 --offline

# Standard workspace test suite (140 passed across 43 suites)
cargo test --workspace -j 2 --offline
```

### 1.2 Multi-Region Broker Ownership Tests
```powershell
cargo test -p kamish-broker --test multiregion_ownership -- --nocapture
```

### 1.3 Full 3-Member Etcd Multi-Region Consensus Suite
```bash
# 1. Download etcd 3.7.3 binary (verified SHA-256)
./scripts/download_etcd.sh

# 2. Generate mutual TLS test certificates (RSA 2048 / pure-Rust TLS)
./scripts/generate_test_certs.sh

# 3. Start local 3-node etcd cluster (ports 2379, 2381, 2383)
./scripts/start_etcd_cluster.sh

# 4. Execute integration suite
./scripts/run_etcd_tests.sh

# 5. Stop cluster cleanly
./scripts/stop_etcd_cluster.sh
```

---

## 2. Deterministic Edge Case Reproduction Steps

### Test 1: Concurrency Race for Epoch 1
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_01_competing_claimants_race_for_epoch_1_repeated`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_01_competing_claimants_race_for_epoch_1_repeated --nocapture
  ```
- **Assertion:** 10 consecutive rounds where Region A and Region B simultaneously propose epoch 1 via CAS. In every round, exactly one claimant succeeds; the loser receives `MetaError::StaleEpoch` and cleans up its unused lease.

### Test 2: Epoch Persistence Across Owner Key Expiry
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_02_epoch_never_regresses_after_owner_key_expiry`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_02_epoch_never_regresses_after_owner_key_expiry --nocapture
  ```
- **Assertion:** Establishes epoch 1 with a short 2s lease. After server lease expiry, `owner_key` disappears from etcd, but `epoch_key` remains 1. A stale proposal at epoch 0 fails; a proposal at epoch 1 -> 2 succeeds.

### Test 3: Lease TTL Expiry Under Paused Client
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_03_lease_ttl_expiry_under_paused_client_and_monotonic_clock`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_03_lease_ttl_expiry_under_paused_client_and_monotonic_clock --nocapture
  ```
- **Assertion:** 5s lease granted. Mock monotonic clock advances 4800ms. Local lease fails closed (`is_valid() == false`) while etcd server still has 200ms remaining, preventing split-brain writes during process pauses.

### Test 4: Lost Keepalive Broker Fencing
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_04_keepalive_lost_broker_stops_acking`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_04_keepalive_lost_broker_stops_acking --nocapture
  ```
- **Assertion:** Simulates network drop preventing keepalive renewals. Local monotonic clock advances past effective lease duration. Broker local write admission immediately ceases.

### Test 5: Policy Floor Advancement & Monotonic Source Generation
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_05_policy_floor_and_source_generation`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_05_policy_floor_and_source_generation --nocapture
  ```
- **Assertion:** Quorum enforces monotonically increasing policy floors (0 -> 10, regression to 9 rejected with `PolicyFloorRegression`). Source identity generation strictly increments across node restarts.

### Test 6: Ambiguous Commit Outcome & Retry Idempotence
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_06_ambiguous_commit_outcome_handling_and_retry_idempotence`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_06_ambiguous_commit_outcome_handling_and_retry_idempotence --nocapture
  ```
- **Assertion:** Region A submits CAS 1 -> 2. Network drop simulated. Retry of proposal 1 -> 2 by Region A succeeds idempotently without error. Replay of proposal 1 -> 2 by competing Region B fails with `StaleEpoch`.

### Test 7: Quorum Loss Fail-Closed & Recovery
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_07_quorum_loss_fail_closed_and_member_restart`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_07_quorum_loss_fail_closed_and_member_restart --nocapture
  ```
- **Assertion:** Member 3 killed: 2/3 quorum holds, CAS succeeds. Member 2 killed: 1/3 alive, quorum lost, write operations fail closed. Members 2 and 3 restarted: cluster heals, writes resume.

### Test 8: Cold Restart Epoch and Fence Persistence
- **File:** `crates/kamish-meta/tests/etcd_authority.rs` (`test_08_cold_restart_epoch_and_fence_persistence`)
- **CLI Command:**
  ```bash
  KAMISH_ETCD_TESTS=1 cargo test -p kamish-meta --test etcd_authority -- test_08_cold_restart_epoch_and_fence_persistence --nocapture
  ```
- **Assertion:** Epoch 42 committed. Entire 3-member cluster stopped with `stop_etcd_cluster.sh`. Entire cluster restarted with `start_etcd_cluster.sh`. Stale CAS at epoch 41 fails; CAS 42 -> 43 succeeds.

### Test 9: MultiRegion Mode Unregistered Stream Rejection
- **File:** `cmd/kamish-broker/tests/multiregion_ownership.rs`
- **CLI Command:**
  ```powershell
  cargo test -p kamish-broker --test multiregion_ownership -- --nocapture
  ```
- **Assertion:**
  1. Produce to unregistered stream in MultiRegion mode returns `Err(BrokerError::PolicyDenied("unregistered_stream_in_multi_region_mode"))`.
  2. Direct `set_ownership` fails closed.
  3. CAS registration allows produce at committed epoch.
  4. Ownership change to another region rejects produce with `BrokerError::PolicyDenied("not_current_partition_owner")`.
  5. Expired lease rejects produce with `BrokerError::PolicyDenied("not_current_partition_owner")`.

---

## 3. Identified Architectural Gaps Reproduction

### Reproduction of Gap 1: Synchronous Network I/O on Produce Path
- **Observation:** `BrokerServer::produce` line 412 calls `self.meta.get_ownership(&topic, partition)`.
- **Under `EtcdMetadataAuthority`:** `crates/kamish-meta/src/etcd.rs:720-725` executes `kv_client.get(owner_k, None).await`.
- **Reproduction:** Sever network connectivity to etcd endpoints while broker has an active, unexpired local lease.
- **Result:** `get_ownership` blocks for the connection timeout (up to 4s) and returns `None`, rejecting client produce even though the local lease was physically valid and unexpired according to the monotonic clock.

### Reproduction of Gap 2: Wall-Clock Skew Exposure in Broker Loop
- **Observation:** `cmd/kamish-broker/src/lib.rs:420` and line 432 execute:
  ```rust
  let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(i64::MAX);
  if owner.owner_region != self.region || now >= owner.lease_expiry_ns {
      return Err(BrokerError::PolicyDenied("not_current_partition_owner".into()));
  }
  ```
- **Reproduction:** Set system clock backward by 10 minutes while a lease is active.
- **Result:** `now` evaluates to a timestamp far in the past, causing `now >= owner.lease_expiry_ns` to evaluate to `false` even after the physical lease duration has elapsed.

