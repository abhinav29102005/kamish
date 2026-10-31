# Kamish — Task Audit Ledger

This ledger records every engineering task, cross-referencing requirements, components, and invariants from [project.md](project.md).

---

## Invariant Reference Table

| Invariant | Description | Verification Method |
|---|---|---|
| **INV-1** | Offsets are dense and monotonic per partition | Ordering checker & Segment index test |
| **INV-2** | An acknowledged record is on a majority of in-region replicas | Majority fsync verification |
| **INV-3** | A new leader holds all acknowledged records | Failover & loss checker |
| **INV-4** | A fetch never returns records beyond committed high-water mark | HWM reader contract test |
| **INV-5** | Destination log is a prefix of source log; offsets match exactly | Prefix & checksum verification |
| **INV-6** | Retrying an applied range is a no-op | Outbox retry & idempotent session test |
| **INV-7** | No two regions commit writes for same partition and epoch | Epoch lease & stale-epoch auditor |
| **INV-8** | No payload outside legal set or sealed-only mode | Broker enforcement & canary scanner |
| **INV-9** | An unsigned, invalid or rolled-back policy is never applied | Ed25519 signature & version test |

---

## Task Audit Log

### TASK-0001: Project Setup & Audit Architecture
* **Timestamp:** 2026-10-31 16:42 UTC
* **Components:** Project Root, Documentation, ADRs
* **Project.md References:** §1.4, §1.5, §3.6, §4.7.1, §4.11
* **Invariants Checked:** None directly violated; foundational setup for INV-1 through INV-9.
* **Actions Taken:**
  1. Established Architecture Decision Records in `decisions.md` (ADR-0001 to ADR-0004).
  2. Created task audit ledger `TASK_AUDIT.md` for continuous verification against `project.md`.
  3. Configured Rust multi-crate workspace adhering to repository structure in §3.6.
* **Verification:** Structure verified against repository manifest in `project.md`.
* **Status:** PASS


### TASK-0002: Implementation of Hash-Chained Decision Audit Log (`kamish-audit`)
* **Timestamp:** 2026-10-31 16:44 UTC
* **Component:** `crates/kamish-audit`
* **Project.md References:** §1.4.1 (FR-13), §2.6, §2.7.5, §3.4.1, §4.7.1
* **Invariants Checked:** INV-8, INV-9
* **Actions Taken:**
  1. Built SHA-256 hash-chained immutable decision log with sequence verification.
  2. Implemented tamper-detection and full chain verification (`verify_integrity()`).
  3. Enforced zero-payload threat model: stores only action, class, regions, policy version, and decision outcome.
* **Verification:** Unit tests verifying genesis link, append progression, disk persistence, and tamper resistance (`test_tamper_detection`).
* **Status:** PASS

### TASK-0003: Sovereignty Policy Engine & Versioning (`kamish-policy`)
* **Timestamp:** 2026-10-31 16:44 UTC
* **Component:** `crates/kamish-policy`
* **Project.md References:** §2.4, §3.3.3, INV-9, FR-01, FR-08
* **Invariants Checked:** INV-9 (tampered / replayed / rolled-back policy rejection)
* **Actions Taken:**
  1. Implemented class rules, legal set validation, and destination modes (`Cleartext`, `SealedOnly`, `Deny`).
  2. Built Ed25519 signing and verification with trusted issuer enforcement.
  3. Implemented policy cache with strict monotonic versioning and staleness expiration.
* **Verification:** Unit tests verifying signature validity, expiry windows, and rollback rejection.
* **Status:** PASS

### TASK-0004: Single Decision Function (`kamish-enforce`)
* **Timestamp:** 2026-10-31 16:45 UTC
* **Component:** `crates/kamish-enforce`
* **Project.md References:** §2.6, §2.7.5, §3.4.1, INV-8, FR-01, FR-08
* **Invariants Checked:** INV-8 (payload restricted to legal set/mode), INV-9 (fail-closed if policy invalid/stale)
* **Actions Taken:**
  1. Built `Enforcer::decide()` mapping §3.4.1 logic across produce, cross-region send, promote, and fetch operations.
  2. Integrated automatic auditing: every single decision is logged into the `kamish-audit` hash chain.
  3. Implemented fail-closed behavior for unknown classes, expired policies, or non-legal regions.
* **Verification:** Unit tests for produce, remote send, non-legal destination rejection, and promotion gating.
* **Status:** PASS

### TASK-0005: Segmented Append-Only Log Storage (`kamish-log`)
* **Timestamp:** 2026-10-31 16:45 UTC
* **Component:** `crates/kamish-log`
* **Project.md References:** §3.5.2, INV-1, INV-4
* **Invariants Checked:** INV-1 (dense monotonic offsets), INV-4 (reads bounded by High-Water Mark)
* **Actions Taken:**
  1. Implemented §3.5.2 record envelope (`magic`, `epoch`, `class_id`, `policy_version`, `base_offset`, `payload`, `checksum`).
  2. Implemented CRC32 checksum verification and epoch fencing checks.
  3. Implemented HWM read bounds and persistent disk recovery.
* **Verification:** Unit tests verifying monotonic offsets, CRC validation, HWM-bounded fetches, and stale-epoch rejection.
* **Status:** PASS

### TASK-0006: Project Renaming to Kamish & Semantic Endpoint Protocol Design
* **Timestamp:** 2026-10-31 16:48 UTC
* **Component:** Entire Workspace & `proto/kamish/stream.proto`
* **Project.md References:** §1.1, §3.5.1, §3.6, ADR-0005
* **Invariants Checked:** INV-1 through INV-9
* **Actions Taken:**
  1. Standardized project naming to **Kamish** across all crates (`kamish-audit`, `kamish-policy`, `kamish-enforce`, `kamish-log`) and binaries (`kamish-broker`, `kamish-agent`, `kamish-ctl`).
  2. Deprecated generic `/api/v1` styles; created domain-specific semantic Protobuf protocol in `proto/kamish/stream.proto`:
     - Data plane: `/kamish.stream.StreamService/Produce`, `/kamish.stream.StreamService/Fetch`
     - Replication plane: `/kamish.replication.ReplicationService/TransferReplicationRange`, `/kamish.replication.ReplicationService/GetDurableWatermark`
     - Governance plane: `/kamish.governance.GovernanceService/PublishPolicy`, `/kamish.governance.GovernanceService/VerifyAuditChain`
     - Failover plane: `/kamish.cluster.ClusterCoordinationService/RenewPartitionLease`, `/kamish.cluster.ClusterCoordinationService/PromotePartitionOwner`
  3. Added ADR-0005 in `decisions.md`.
* **Verification:** Full workspace build and test pass (`cargo test --workspace`).
* **Status:** PASS

### TASK-0007: Comprehensive Enforcement Points E1-E10 Mapping
* **Timestamp:** 2026-10-31 16:56 UTC
* **Components:** `crates/kamish-audit`, `crates/kamish-enforce`
* **Project.md References:** §2.6, §2.7.5, Table 2.6, INV-8, FR-12
* **Invariants Checked:** INV-8 (no payload outside legal set/sealed mode), FR-12 (transit relay for ciphertext only)
* **Actions Taken:**
  1. Extended `AuditAction` to cover E1 through E10 (Ingress, BrokerAppend, InRegionReplica, ReplicationSender, ReplicationReceiver, BackupExport, ConsumerFetch, Promote, TelemetryExport, AdminReplay, RelayTransit).
  2. Implemented strict ciphertext enforcement for relay transit (E10): cleartext transit attempts are rejected with `transit_denied_for_cleartext`.
* **Verification:** Unit tests covering E1, E4, and E10 cleartext rejection.
* **Status:** PASS

### TASK-0008: Metadata Entity Store (`kamish-meta`)
* **Timestamp:** 2026-10-31 16:56 UTC
* **Component:** `crates/kamish-meta`
* **Project.md References:** §2.5, §2.2, INV-7
* **Invariants Checked:** INV-7 (no two regions commit writes for same partition and epoch)
* **Actions Taken:**
  1. Built thread-safe metadata store holding topics, partitions, ownership, and replicas.
  2. Implemented CAS epoch promotion: bumping partition epoch monotonically upon verified failover.
  3. Built durable watermark tracking per remote replica.
* **Verification:** Unit tests verifying epoch fencing CAS and watermark monotonicity.
* **Status:** PASS

### TASK-0009: Durable Outbox & Watermark Trimming (`kamish-outbox`)
* **Timestamp:** 2026-10-31 16:57 UTC
* **Component:** `crates/kamish-outbox`
* **Project.md References:** §2.2, §2.7.3, §3.3.2, INV-5, INV-6
* **Invariants Checked:** INV-5 (prefix matching), INV-6 (idempotent duplicate retry)
* **Actions Taken:**
  1. Implemented bounded queues per destination region with backpressure rules (Block, Reject, DegradeLocal).
  2. Implemented watermark advancement: ranges strictly below durable watermark are trimmed from the queue.
  3. Handled duplicate acks idempotently without state corruption.
* **Verification:** Unit tests for bounded queue limits, watermark advancement, and duplicate no-op acks.
* **Status:** PASS

### TASK-0010: WAN-Adaptive Route Selector (`kamish-route`)
* **Timestamp:** 2026-10-31 16:57 UTC
* **Component:** `crates/kamish-route`
* **Project.md References:** §2.2, §2.7.4, §3.3.4, §3.4.3, NFR-04
* **Invariants Checked:** INV-8 (hard policy filter before routing), NFR-04 (hysteresis & dwell time)
* **Actions Taken:**
  1. Implemented two-stage route picker: hard policy filtering FIRST across all hops, followed by candidate path scoring.
  2. Implemented §3.4.3 cost calculation (`W1*RTT + W2*Loss + W3*QueueDelay + W4*Cost + W5*CongestionRisk`).
  3. Integrated 15% hysteresis margin and 5000ms dwell window to prevent routing oscillations.
* **Verification:** Unit tests verifying illegal fast paths are rejected in favor of legal permitted paths.
* **Status:** PASS

### TASK-0011: Idempotent Producer Session Deduplication (`kamish-log`)
* **Timestamp:** 2026-10-31 17:23 UTC
* **Component:** `crates/kamish-log`
* **Project.md References:** §3.4.2, INV-6, FR-02
* **Invariants Checked:** INV-6 (retrying an applied range is a no-op), FR-02 (idempotent producer sessions)
* **Actions Taken:**
  1. Implemented `SessionManager` tracking `(producer_id, epoch) -> (last_seq, last_offset)`.
  2. Implemented sequence monotonicity verification: rejects out-of-order sequence numbers.
  3. Implemented duplicate retry handling: returns recorded offset without appending new duplicate data.
* **Verification:** Unit tests verifying sequence increments, duplicate offset returns, and out-of-order rejection.
* **Status:** PASS

### TASK-0012: Partition Ownership Coordinator & Quarantine Store (`kamish-own`)
* **Timestamp:** 2026-10-31 17:24 UTC
* **Component:** `crates/kamish-own`
* **Project.md References:** §3.1.3, §3.1.4, §3.3.1, §3.4.4, §4.7.2 (R6), INV-3, INV-7, FR-08
* **Invariants Checked:** INV-3 (leader holds acknowledged records), INV-7 (no split-brain / stale writes)
* **Actions Taken:**
  1. Implemented `try_promote` incorporating lease expiry check with `max_clock_drift` buffer.
  2. Integrated Enforcer `PROMOTE` action check against policy `failover_candidates`.
  3. Implemented writer fencing rejecting any write with epoch lower than partition epoch.
  4. Implemented `QuarantineStore` safely isolating superseded suffixes of returning old leaders.
* **Verification:** Unit tests for lease-active failure, expired lease promotion, stale writer fencing, and record quarantine.
* **Status:** PASS

### TASK-0013: Simulated Key Service & Sealed Copy Encryption (`kamish-seal`)
* **Timestamp:** 2026-10-31 17:24 UTC
* **Component:** `crates/kamish-seal`
* **Project.md References:** §1.6 D2, §2.4, §3.4.5, §3.5.2, §4.7.1
* **Invariants Checked:** INV-8 (no cleartext leakage to remote regions)
* **Actions Taken:**
  1. Built `RegionalKeyService` strictly enforcing that keys reside within home region.
  2. Built `SealEngine` authenticated encryption generating `SealedEnvelope`.
  3. Proved that foreign regions cannot decrypt sealed copies without home key service access.
* **Verification:** Unit tests verifying sealed ciphertext encryption and cross-border key denial.
* **Status:** PASS

### TASK-0014: Automated Invariant Checkers & Canary Scanner (`kamish-checkers`)
* **Timestamp:** 2026-10-31 17:24 UTC
* **Component:** `crates/kamish-checkers`
* **Project.md References:** §1.4.1 FR-10, §3.5.3, §4.4, INV-1, INV-7, INV-8
* **Invariants Checked:** INV-1 (dense monotonic offsets), INV-7 (epoch conflicts), INV-8 (canary leaks)
* **Actions Taken:**
  1. Built `OrderingChecker` catching gaps in partition offset progression.
  2. Built `EpochAuditor` detecting stale epoch writes or split-brain commits.
  3. Built `CanaryScanner` identifying leak of synthetic canary tokens into forbidden regions.
* **Verification:** Unit tests verifying ordering gap detection and canary leak detection.
* **Status:** PASS
