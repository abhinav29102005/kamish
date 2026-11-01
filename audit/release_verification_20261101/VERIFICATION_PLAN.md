# KAMISH — FULL VERIFICATION, DISTRIBUTED CORRECTNESS & RELEASE READINESS PLAN

**Run Identifier:** `run-verification-20261101-113645`  
**Target Repository:** `E:\Kamish`  
**Starting Commit:** `b4313b30a4d8d5e18c70f5551bf6137055397616` (Branch: `main`)  
**Audit Directory:** `audit/release_verification_20261101/`  
**Date:** 2026-11-01  

---

## 1. Objectives & Scope

This campaign performs an exhaustive verification and defect remediation across the Kamish distributed messaging codebase.
Unlike implementation phases, this phase explicitly executes all unit and integration test suites, runs distributed consensus scenarios, verifies crash consistency, exercises security boundaries, validates consumer group lifecycles, checks operational endpoints, and audits benchmark prerequisites.

---

## 2. Requirement-to-Evidence Matrix

| Category | Requirement ID | Focus Area | Planned Verification Method | Expected Evidence |
|:---|:---|:---|:---|:---|
| **Storage & Log** | `LOG-01` to `LOG-06` | Log segments, sparse indexing, CRC32, atomic truncation, prefix retention, active tail preservation | `cargo test -p kamish-log`, dedicated retention lifecycle tests | Zero corruption, index alignment, monotonic offset preservation |
| **Durability & Quorum** | `DUR-01`, `DUR-02` | `LeaderFsync` and `InRegionQuorum` ($M = \lfloor V/2 \rfloor + 1$) durability | Multi-broker TCP integration tests, quorum loss & recovery tests | No uncommitted reads (INV-4), committed history immutability |
| **Deduplication** | `DED-01` | Producer session deduplication (INV-6, FR-02) | Sequence check and idempotent retry tests | Duplicates return cached offset; conflicting sequence fails |
| **Metadata & Leases** | `OWN-01` to `OWN-05` | etcd 3.7.3 ownership leases, 2.0s margin, proactive renewal, epoch fencing | Lease renewal tests, epoch fencing tests, promote tests | Fenced writes rejected; renewal failure cancels authority |
| **Replication & Outbox** | `OUT-01` to `OUT-03`, `REP-01` to `REP-05` | Outbox capacity bounds, provenance watermarks, divergent history quarantine, 500ms heartbeat sweeps | `cargo test -p kamish-outbox`, replication convergence tests | Backpressure on capacity; conflicting suffix to QuarantineStore |
| **Routing & Breakers** | `ROU-01`, `ROU-02` | Adaptive latency route selection, circuit breaker trips | `cargo test -p kamish-route` | Circuit opens on repeated failures, routes via lowest latency |
| **Consumer Groups** | `CNS-01` to `CNS-04` | Dynamic membership, round-robin assignment, partition ownership validation, session sweeper, generation fencing | `cargo test -p kamish-broker --test consumer_group*`, SDK tests | Zombie members evicted; unassigned partition commit rejected |
| **Security & Custody** | `SEC-01` to `SEC-07` | Ring AEAD (ChaCha20/AES-256), OsRng nonces, tenant AAD, fail-closed KMS, mTLS, wire RBAC, Ed25519 audit witnessing | `cargo test -p kamish-seal`, `kamish-audit`, wire RBAC tests | Ciphertext/tag mismatch fails; unconfigured KMS fails closed |
| **SDK & Protocol** | `SDK-01` to `SDK-03` | TypeScript SDK transport pool, request framing, producer sequence, consumer group coordination | TypeScript test runner / syntax & typecheck | Proper JSON/TCP framing and type safety |
| **Observability & Ops** | `OPS-01` to `OPS-03` | Prometheus `/metrics` (configured, available, committed quorum), graceful shutdown, benchmark orchestrator | Gateway integration test, benchmark orchestrator validation | Metrics match live state; graceful shutdown drains tasks |

---

## 3. Execution Schedule & Workflow

1. **Step 1: Workspace Test Suite Baseline Execution:** Run `cargo test --workspace` and capture exact baseline results across all crates.
2. **Step 2: Defect Remediation & Targeted Re-verification:** Analyze any test failures, fix root causes in source code without weakening safety checks, re-verify with targeted checks, and commit fixes.
3. **Step 3: In-Region Quorum & Distributed Consensus Verification:** Verify multi-node TCP consensus under quorum formation, minority partition, leader crash, follower catch-up, and commit gating.
4. **Step 4: Storage Crash Consistency & Retention Verification:** Verify prefix reclamation, active segment tail preservation, and quarantine pruning.
5. **Step 5: Security & Cryptographic Boundary Verification:** Verify AEAD round trips, AAD tenant tampering, OsRng nonces, fail-closed KMS boundaries, and deterministic audit IDs.
6. **Step 6: Consumer Group Lifecycle Verification:** Verify dynamic rebalancing, heartbeat expiry, partition assignment ownership, and restart recovery.
7. **Step 7: Gateway, SDK & Benchmark Integration:** Verify REST endpoints, live Prometheus telemetry, TypeScript SDK, and run benchmark orchestrator dependency validation.
8. **Step 8: Final Evidence Package & Documentation:** Update authoritative documents and produce the final verification report.

