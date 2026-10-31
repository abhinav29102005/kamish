# KAMISH MILESTONE 5.4 AUDIT REPORT
## Metadata Ownership Durability & Epoch Fencing Audit

**Date:** 2026-11-01  
**Repository:** `E:\Kamish`  
**Git Baseline Commit:** `27cbc49fd14f32f71a8be513b0eca7f81b796ebc`  
**Branch:** `main`  
**Audit Verdict:** `CONDITIONAL — REPRODUCIBLE GAPS REMAIN`

---

## 1. Executive Summary

Milestone 5.4 conducts a comprehensive distributed systems audit of partition ownership, lease management, persistence boundaries, and epoch fencing across the Kamish codebase.

The audit evaluated two primary operational architectures present in the codebase:
1. **MultiRegion Architecture (`EtcdMetadataAuthority`):** Backed by an authoritative `etcd 3.7.3` consensus quorum, pure-Rust mTLS (`tls-ring`), atomic single-Txn compare-and-swap (CAS), non-lease-attached persistent epoch keys (`kamish/streams/{topic}/{partition}/epoch`), and local monotonic clock lease validation with drift and transit allowances.
2. **SingleNodeCompatibility Architecture (`MetadataStore`):** In-memory prototype store intended for isolated local development, where partition ownership and leases reside purely in volatile heap memory (`HashMap<(String, u32), PartitionOwnership>`).

### Key Audit Findings:
- **Strengths Established:**
  - **Atomic Multi-Region CAS:** `EtcdMetadataAuthority::compare_and_swap_epoch` executes atomic etcd transactions with strict version/value equality comparisons, preventing concurrent claimants from acquiring the same epoch.
  - **Persistent Epoch Fences:** Epoch keys in etcd are not attached to etcd leases and persist indefinitely across lease expirations, client disconnections, and whole-cluster cold restarts (verified by `test_08_cold_restart_epoch_and_fence_persistence`).
  - **Local Monotonic Expiry:** `LocalLease` evaluates expiry against a monotonic clock (`MonotonicClock`) with conservative drift deductions (`max_clock_drift_ms = 200`, `transit_delay_allowance_ms = 100`), ensuring a paused broker ceases write admission before server-side lease expiration.
  - **Fail-Closed Multi-Region Ingress:** In `DeploymentMode::MultiRegion`, produce requests for unregistered or unverified partitions fail closed (`BrokerError::PolicyDenied("unregistered_stream_in_multi_region_mode")`).
  - **Append Boundary Fencing:** `BrokerServer::produce` performs lease validity checks both prior to file modification and immediately following WAL append, preventing successful client ACKs on torn or expired leases.

- **Reproducible Gaps Identified:**
  - **Gap 1 (NFR-01 Write Path Ingress):** `EtcdMetadataAuthority::get_ownership()` executes a synchronous gRPC KV read (`kv_client.get(owner_k, None)`) to etcd on every produce call. This violates NFR-01 ("Metadata quorum is NOT consulted on the local write path"), adding cross-region network latency and failing closed when etcd experiences transient communication drops.
  - **Gap 2 (Wall-Clock Exposure in Broker Server):** While `LocalLease` internally uses `MonotonicClock`, `BrokerServer::produce` evaluates lease expiry via `now = chrono::Utc::now().timestamp_nanos_opt() >= owner.lease_expiry_ns`. This re-exposes the broker write loop to system wall-clock skew and NTP backward adjustments.
  - **Gap 3 (Unchecked CAS Takeover / Dual-Primary Hazard):** `EtcdMetadataAuthority::try_compare_and_swap_epoch_once` verifies only `epoch_key == expected_epoch`. It does not verify that the previous owner's lease has expired or that a safe clock-drift buffer has elapsed. While `PartitionOwnerCoordinator` in `crates/kamish-own` implements this check (`try_promote`), `BrokerServer::claim_partition_ownership` directly invokes CAS on the metadata authority, bypassing the safe expiry margin.
  - **Gap 4 (Single-Node Silent Epoch Fallback):** In `DeploymentMode::SingleNodeCompatibility`, unregistered streams silently default to epoch 1 (`ownership.as_ref().map_or(1, |owner| owner.epoch)`). If an in-memory `MetadataStore` restarts, it loses ownership records and falls back to epoch 1, relying solely on local WAL segment scanning (`PartitionLog::open`) to prevent epoch regression.

---

## 2. Ownership Lifecycle Trace

The 10 mandatory ownership lifecycle stages were audited across `crates/kamish-meta/src/lib.rs`, `crates/kamish-meta/src/etcd.rs`, `cmd/kamish-broker/src/lib.rs`, and `crates/kamish-own/src/lib.rs`:

| Lifecycle Stage | Implementation Path | Authoritative State | Persistence Boundary | Failure & Boundary Behavior |
| :--- | :--- | :--- | :--- | :--- |
| **1. Registration** | `EtcdMetadataAuthority::compare_and_swap_epoch` | etcd Raft log & BoltDB | Consensus Quorum | Requires `expected_epoch == 0` (or `version == 0`). Registers `OwnerRecord` under lease and sets initial epoch. |
| **2. Lease Creation & Expiry** | `etcd::LeaseClient::grant` & `LocalLease::from_grant` | etcd lease manager & Local monotonic cache | Memory (Client) + Consensus (Server) | Client calculates `effective_duration = duration - 300ms`. Client fails closed when local monotonic elapsed time exceeds effective duration. |
| **3. Renewal** | `MetadataAuthority::renew_lease` | etcd `lease_client.keep_alive` | Consensus Quorum | If keepalive drops, client lease expires locally; server expires `owner_key` upon TTL expiry. |
| **4. Transfer / Reassignment** | `PartitionOwnerCoordinator::try_promote` & CAS | etcd Txn | Consensus Quorum | `try_promote` checks `now_ns >= safe_expiry_ns` (lease + drift margin) and policy eligibility before executing CAS `epoch -> epoch + 1`. |
| **5. Epoch Advancement** | CAS `epoch_key` put | etcd key `.../epoch` | Persistent (no lease) | Monotonically increments. Never regresses even if owner crashes or lease lapses. |
| **6. Produce Authorization** | `BrokerServer::produce` | `self.meta.get_ownership` | In-memory `LocalLease` + etcd KV | Fails closed if not current owner region or if lease expired. Pre- and post-append verification. |
| **7. Broker Restart** | `BrokerServer::try_new` & `PartitionLog::open` | On-disk WAL (`partition.data`) + etcd | Disk + Consensus | WAL recovers highest seen epoch (`recovered_epoch`). In MultiRegion mode, produce fails closed until consensus ownership is verified. |
| **8. Metadata Service Restart** | etcd cluster restart (`run_etcd_tests.sh`) | etcd Raft data directory | Multi-node Disk Quorum | `epoch_key` survives cluster cold restart. Stale CAS rejected; advancement from persisted epoch succeeds. |
| **9. Stale-Owner Handling** | Pre/post append lease checks & `PartitionLog::append` | Local log writer lock + `MetadataAuthority` | Disk + Client Fencing | Old owner rejected with `not_current_partition_owner` or `lease_expired_outcome_unknown`. Log rejects epoch < recovered. |
| **10. Unavailability Recovery** | `EtcdMetadataAuthority::ensure_connected` | gRPC reconnection loop | Client Runtime | Broker fails closed during outage; automatically reconnects with backoff and resumes ownership verification upon quorum recovery. |

---

## 3. Metadata Persistence Audit

### 3.1 Memory vs. Disk vs. Consensus Boundaries
- **Volatile State (`MetadataStore`):** Process heap memory. When the process exits or crashes, all registered topics, ownerships, and leases are instantly lost. No recovery mechanism exists without external orchestration.
- **Durable Local Storage (`PartitionLog`):** Appends `LogRecord` instances to `partition.data` with an envelope containing `epoch: u64`, `class_id: String`, `policy_version: u64`, `base_offset: u64`, and CRC32. On restart, `PartitionLog::open` scans records and sets `current_epoch = max(recovered_epoch, initial_epoch)`.
- **Durable Distributed Consensus (`EtcdMetadataAuthority`):** Ownership state is persisted across a 3-node etcd cluster using the Raft consensus algorithm. Two distinct keys govern each stream:
  1. `kamish/streams/{topic}/{partition}/owner`: Attached to an etcd lease. Automatically expires if the holder dies or loses connectivity.
  2. `kamish/streams/{topic}/{partition}/epoch`: Never attached to a lease. Retained permanently across cluster restarts.

### 3.2 Cold Restart Analysis
- **MetadataStore Restart:** Starts with an empty `HashMap`. In `SingleNodeCompatibility` mode, produce calls default to epoch 1. If the local log contains records at epoch 5, `PartitionLog::append_with_context` catches the regression (`LogError::StaleEpoch { current: 5, requested: 1 }`). However, metadata-level knowledge of the owner region and lease duration is completely lost.
- **EtcdMetadataAuthority Cold Restart:** Evaluated in `test_08_cold_restart_epoch_and_fence_persistence`. When all 3 etcd nodes are cleanly stopped and restarted, `epoch_key` retains its committed value (e.g., epoch 42). A stale proposal at epoch 41 fails with `StaleEpoch`, while a valid bump to epoch 43 succeeds immediately.

---

## 4. Stale-Owner Fencing Audit

### 4.1 Fencing Mechanism Evaluation
1. **Network Partition of Owner:** If Broker A (epoch E) is partitioned from etcd, its local monotonic lease clock continues running. After `effective_duration_ms` (lease duration minus 300ms safety buffer), `LocalLease::is_valid()` returns `false`. Broker A rejects incoming produce requests with `unregistered_stream_in_multi_region_mode`.
2. **Concurrent Reassignment:** If Broker B claims epoch E+1 via CAS, etcd updates `owner_key` and bumps `epoch_key`. When Broker A checks `get_ownership()`, it receives Broker B's ownership and rejects produce requests with `BrokerError::PolicyDenied("not_current_partition_owner")`.
3. **Mid-Flight Expiration (Torn Append Race):** If Broker A's lease expires while `PartitionLog::append_with_context` is actively executing `fsync()`, Broker A's post-append check in `BrokerServer::produce` (lines 431–440) detects that `now >= owner.lease_expiry_ns`. It refuses to acknowledge the write to the producer, returning `BrokerError::PolicyDenied("lease_expired_outcome_unknown")`.

---

## 5. Concurrency & Restart Ordering Audit

### 5.1 Concurrency Linearization Points
- **Ownership Mutation Linearization Point:** The Raft commit index of the single atomic etcd transaction in `EtcdMetadataAuthority::try_compare_and_swap_epoch_once`.
- **Local Write Admission Linearization Point:** Acquisition of the `partition_operations` mutex stripe in `BrokerServer::produce`, followed by atomic evaluation of the local lease.
- **Durable Append Linearization Point:** Acquisition of `PartitionLog::writer_lock` and the subsequent `File::sync_all()` syscall.

### 5.2 Concurrency Racing Edge Cases
- **CAS Claim Race:** Tested in `test_01_competing_claimants_race_for_epoch_1_repeated`. When Region A and Region B concurrently propose epoch 1 across 10 consecutive rounds, exactly one claimant succeeds in every round; the loser consistently receives `MetaError::StaleEpoch` and cleans up its unused lease.
- **Direct Broker Reassignment Race:** If `BrokerServer::claim_partition_ownership` is invoked directly without `PartitionOwnerCoordinator::try_promote`, an administrator could theoretically bump the epoch in etcd before the old owner's physical lease duration has elapsed.

---

## 6. Required Failure Contract

| Scenario | System Response | Conformance Status |
| :--- | :--- | :--- |
| **Quorum Loss (< 2/3 etcd alive)** | Write requests fail closed (`etcd Txn` and `kv.get` fail; produce rejects). | **VERIFIED** (`test_07_quorum_loss_fail_closed_and_member_restart`) |
| **Metadata Authority Unreachable** | Broker enters fail-closed state (`unregistered_stream_in_multi_region_mode`). | **VERIFIED** |
| **Local Lease Expired** | Immediate rejection of write requests (`not_current_partition_owner`). | **VERIFIED** (`test_03_lease_ttl_expiry_under_paused_client`) |
| **Torn Append on Expired Lease** | Unacknowledged to client (`lease_expired_outcome_unknown`). | **VERIFIED** (`cmd/kamish-broker/src/lib.rs:436`) |
| **Unregistered Stream in MultiRegion** | Immediate rejection; no silent epoch assumption. | **VERIFIED** (`cmd/kamish-broker/tests/multiregion_ownership.rs:114`) |

---

## 7. Invariant Evidence Matrix (M1 – M8)

| Invariant | Description | Status | Evidence & Test Citations |
| :--- | :--- | :--- | :--- |
| **M1** | Ownership epochs do not silently regress after restart | **PASS** | `crates/kamish-meta/tests/etcd_authority.rs:test_08_cold_restart_epoch_and_fence_persistence`; `crates/kamish-log/src/lib.rs:409-413` (`recovered_epoch`). |
| **M2** | A stale owner cannot commit writes after its authority is revoked | **PASS** | `crates/kamish-meta/tests/etcd_authority.rs:test_03_lease_ttl_expiry_under_paused_client_and_monotonic_clock`; `cmd/kamish-broker/src/lib.rs:420-440`. |
| **M3** | Concurrent ownership changes have a well-defined outcome | **PASS** | `crates/kamish-meta/tests/etcd_authority.rs:test_01_competing_claimants_race_for_epoch_1_repeated` (10/10 rounds atomic). |
| **M4** | Lease expiry and renewal are handled consistently | **PARTIAL** | `crates/kamish-meta/src/etcd.rs:773-793` uses monotonic clock; however, `cmd/kamish-broker/src/lib.rs:420` uses wall-clock `Utc::now()`. |
| **M5** | Metadata unavailability cannot silently weaken fencing | **PASS** (MultiRegion) / **CONDITIONAL** (SingleNode) | In `MultiRegion`, unregistered/unavailable streams fail closed (`multiregion_ownership.rs:114`). In `SingleNodeCompatibility`, defaults to epoch 1 (`map_or(1, ...)`). |
| **M6** | WAL epoch recovery does not incorrectly substitute for authoritative ownership | **PASS** | `cmd/kamish-broker/src/lib.rs:412-426` mandates authority ownership match even if WAL has existing segments. |
| **M7** | Restart ordering cannot enable stale-owner writes | **PASS** | If broker restarts while etcd is down, produce calls fail closed (`unregistered_stream_in_multi_region_mode`). |
| **M8** | Ownership validation remains consistent across relevant write paths | **PARTIAL** | `BrokerServer::produce` validates ownership, but `claim_partition_ownership` bypasses the clock-drift buffer in `PartitionOwnerCoordinator`. |

---

## 8. Final Decision Gate

**Selected Gate:** `CONDITIONAL — REPRODUCIBLE GAPS REMAIN`

### Justification:
The implementation of `EtcdMetadataAuthority` demonstrates robust distributed consensus primitives: pure-Rust TLS, non-lease-attached epoch persistence, atomic single-Txn CAS, and monotonic clock lease deductions. All 140 workspace unit and integration tests pass cleanly with zero Clippy warnings.
However, **four concrete architectural gaps remain**:
1. `BrokerServer::produce` performs synchronous etcd KV reads on every produce request, violating NFR-01.
2. `BrokerServer::produce` relies on wall-clock `Utc::now()` for lease expiration checks rather than passing monotonic clock readings.
3. `BrokerServer::claim_partition_ownership` bypasses the clock drift buffer check implemented in `crates/kamish-own`.
4. `SingleNodeCompatibility` mode silently assumes epoch 1 for unregistered streams when metadata is missing.

Remediation must not proceed until authorized.

