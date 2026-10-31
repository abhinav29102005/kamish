# Hydra Stream — Architecture Decision Records (ADRs)

All engineering decisions in this repository are recorded here and continuously verified against [project.md](project.md).

---

## Decision Log Summary

| ADR ID | Date | Title | Status | Related `project.md` Sections |
|---|---|---|---|---|
| **ADR-0001** | 2026-10-31 | Implementation Language: Rust | Accepted | §1.4, §1.5, §3.6, §4.7.2 (R2) |
| **ADR-0002** | 2026-10-31 | Cargo Workspace & Module Architecture | Accepted | §3.6 (Repository & Module Layout) |
| **ADR-0003** | 2026-10-31 | Task Audit Ledger & Continuous Spec Checking | Accepted | §1.4.1 (FR-10, FR-13), §3.5.3 (Invariants), §4.11 |
| **ADR-0004** | 2026-10-31 | Hash-Chained Audit Log Design (`hydra-audit`) | Accepted | §1.4.1 (FR-13), §2.6, §2.7.5, §3.4.1, §4.7.1 |

---

## ADR-0001: Implementation Language: Rust

### Context
Hydra Stream is a sovereignty-aware, WAN-adaptive distributed event broker. The system requires:
1. Low and deterministic latency under high write loads.
2. Direct control over segmented append-only disk I/O, `fsync`, and memory mappings.
3. Memory safety without stop-the-world garbage collection pauses, which could violate lease timings (§3.4.4, §4.7.2 R6).
4. Strict type safety and compile-time enforcement of data invariants (INV-1 through INV-9).

### Decision
Implement the entire Hydra Stream broker, agent, CLI, and core libraries in **Rust (2024 edition / stable toolchain)**.

### Compliance with `project.md`
- **§1.5 (Build, Reuse, and Tiers):** Components 1 to 6 (Segmented log, in-region replication, API, idempotence, outbox) and 9, 11, 13 will be built cleanly in Rust.
- **§4.7.2 (R2, R6):** Prevents buffer overflows, data races, and unpredicted GC pauses that could trigger spurious lease expiration.

---

## ADR-0002: Cargo Workspace & Module Architecture

### Context
`project.md` §3.6 prescribes a modular layout separating binaries (`cmd/`), internal libraries (`internal/` / `crates/`), tests (`test/`), scripts (`scripts/`), and evaluation results (`results/`).

### Decision
Use a multi-crate Cargo workspace rooted at the repository top-level:
- `cmd/hydra-broker`: The regional event broker daemon.
- `cmd/hydra-agent`: The inter-region replication sender and receiver agent.
- `cmd/hydra-meta`: Metadata voter daemon (Raft consensus).
- `cmd/hydra-ctl`: Administrative CLI for policy publishing and cluster inspection.
- `crates/hydra-audit` (aliased in `internal/audit`): Hash-chained cryptographic decision log.
- `crates/hydra-policy` (aliased in `internal/policy`): Ed25519 policy signing, verification, and schema.
- `crates/hydra-enforce` (aliased in `internal/enforce`): The single decision function `enforce()`.
- `crates/hydra-log` (aliased in `internal/log`): Segmented append-only log with sparse index and recovery.
- `crates/hydra-outbox` (aliased in `internal/outbox`): Bounded durable replication queues and watermark tracking.
- `crates/hydra-route` (aliased in `internal/route`): Measurement, graph filter, scoring, and hysteresis.
- `crates/hydra-own` (aliased in `internal/own`): Partition epoch leases, fencing, and quarantine segments.
- `crates/hydra-seal` (aliased in `internal/seal`): Ciphertext-only encryption and key service.
- `crates/hydra-checkers` (aliased in `test/checkers`): Independent loss, duplicate, order, canary, and stale-epoch checkers.

### Compliance with `project.md`
- Matches §3.6 module-for-module.

---

## ADR-0003: Task Audit Ledger & Continuous Spec Checking

### Context
To maintain 100% fidelity to `project.md` throughout execution, every task must be audited against the specification's requirements (FR-01 to FR-14), non-functional constraints (NFR-01 to NFR-08), and core invariants (INV-1 to INV-9).

### Decision
Maintain a formal [TASK_AUDIT.md](TASK_AUDIT.md) log in the repository root. Every engineering task performed must log:
1. Task ID and Timestamp.
2. Target Component / Crate.
3. Addressed Requirements & Invariants from `project.md`.
4. Verification Method (unit test, checker run, or benchmark).
5. Invariant Status (Pass/Fail).

### Compliance with `project.md`
- Fulfills **§4.11 (Claims Checklist)**: Guarantees that every claim has a corresponding test and run log, and prevents drift from specification.

---

## ADR-0004: Hash-Chained Audit Log Design (`hydra-audit`)

### Context
Requirement **FR-13 (P1)** and §2.6 / §2.7.5 specify a hash-chained, region-local decision log. The threat model (§4.7.1) explicitly states:
> *"Telemetry export policy, decision log stores decisions and versions, never payload."*

### Decision
1. Implement `hydra-audit` as a standalone, zero-payload audit library using SHA-256 hash chains.
2. Each `AuditEntry` stores:
   - `sequence`: Strictly increasing 64-bit integer.
   - `timestamp_ns`: UTC nanoseconds timestamp.
   - `prev_hash`: 32-byte SHA-256 hash of the preceding entry (genesis entry uses 32 zero bytes).
   - `action`: Operation being audited (`PRODUCE`, `SEND_REGION`, `PERSIST_REMOTE`, `PROMOTE`, `FETCH`, `REPLAY`).
   - `class_id`: String data classification identifier.
   - `source_region`: Originating region.
   - `target_region`: Target destination region.
   - `policy_version`: Policy version used to evaluate the decision.
   - `decision`: `ALLOW_CLEARTEXT`, `ALLOW_SEALED`, `DENY`.
   - `reason`: Machine-readable reason string (e.g. `ok`, `not_in_legal_set`, `stale_policy`, `invalid_signature`).
   - `entry_hash`: SHA-256 hash of all fields above combined with `prev_hash`.
3. Support persistence to append-only binary files and in-memory verification `verify_chain()` to prove tamper-resistance.
4. **Never** include payload bytes or customer message data in the log.

### Compliance with `project.md`
- **FR-13:** Hash-chained, region-local decision log.
- **§2.6 & §2.7.5:** Audit sink D5 receives decision events from enforcement point P26.
- **§4.7.1:** Telemetry & audit logs do not store regulated payload data.

---

## ADR-0005: Project Naming (Kamish) and Semantic Endpoint Architecture

### Context
1. The project name is formally **Kamish**.
2. Standard REST/RPC generic routing prefixes like `/api/v1/...` obscure protocol intent, lack clear domain boundaries, and complicate strict broker-side sovereignty and auditing boundaries.
3. Kamish requires explicit, domain-driven RPC endpoints for streams, inter-region replication, governance, and cluster failover.

### Decision
1. Standardize all crates, binaries, specifications, and configurations under the **Kamish** identity (`kamish-broker`, `kamish-agent`, `kamish-ctl`, `crates/kamish-*`).
2. Replace generic `/api/v1/...` routes with semantic, domain-bounded RPC endpoints defined in [proto/kamish/stream.proto](proto/kamish/stream.proto):
   - **Stream Service:**
     - `Produce`: `/kamish.stream.StreamService/Produce`
     - `Fetch`: `/kamish.stream.StreamService/Fetch`
     - `GetPartitionMetadata`: `/kamish.stream.StreamService/GetPartitionMetadata`
   - **Replication Service:**
     - `TransferReplicationRange`: `/kamish.replication.ReplicationService/TransferReplicationRange`
     - `GetDurableWatermark`: `/kamish.replication.ReplicationService/GetDurableWatermark`
   - **Governance & Sovereignty Service:**
     - `PublishPolicy`: `/kamish.governance.GovernanceService/PublishPolicy`
     - `GetActivePolicy`: `/kamish.governance.GovernanceService/GetActivePolicy`
     - `VerifyAuditChain`: `/kamish.governance.GovernanceService/VerifyAuditChain`
   - **Cluster Coordination & Failover Service:**
     - `RenewPartitionLease`: `/kamish.cluster.ClusterCoordinationService/RenewPartitionLease`
     - `PromotePartitionOwner`: `/kamish.cluster.ClusterCoordinationService/PromotePartitionOwner`

### Compliance with `project.md`
- **§2.6 (Enforcement Points Map):** E1 (Gateway) and E2 (Broker append) map to `Produce`, E3/E4 map to `TransferReplicationRange`, E5 maps to `Fetch`, E8 maps to `PromotePartitionOwner`.
- **§3.5.1:** Protobuf definitions use explicit domain semantics rather than generic paths.

---

## ADR-0006: Metadata Store & Comprehensive Enforcement Points E1-E10

### Context
`project.md` Part 2 details:
1. §2.5 Metadata Entity-Relationship Model (Topic, Partition, Ownership, Replica, OutboxStream) with monotonic epochs and lease bounds.
2. §2.6 Enforcement Points Map: Every byte movement boundary must call `enforce()` across all 10 operations:
   - E1 (Ingress), E2 (BrokerAppend), E3 (InRegionReplica), E4 (ReplicationSender), E5 (ReplicationReceiver), E6 (BackupExport), E7 (ConsumerFetch), E8 (Promote), E8-Alt (TelemetryExport), E9 (AdminReplay), E10 (RelayTransit).

### Decision
1. Implement `kamish-meta` providing thread-safe, compare-and-swap epoch promotion and durable watermark tracking.
2. Extend `kamish-enforce` and `kamish-audit` to evaluate and audit all 10 operations mapped in §2.6.

### Compliance with `project.md`
- **§2.5 & §2.6:** Exact contract alignment.
- **INV-7 & INV-8:** Fenced ownership and complete movement boundary checking.

---

## ADR-0007: Durable Outbox & Permitted-Graph Route Scoring

### Context
`project.md` §2.2, §2.7.3, §2.7.4, and §3.4.3 specify:
1. Per-destination bounded durable outbox queues with backpressure rules (`Block`, `Reject`, `DegradeLocal`) and resumable watermarks.
2. Route selector filtering to the policy-permitted graph first, then optimizing inside it using the cost scoring function with hysteresis margin and circuit breaker protection.

### Decision
1. Implement `kamish-outbox` managing bounded queues per destination region with watermark advancement and automatic trimming.
2. Implement `kamish-route` executing `enforce()` against all direct and relay paths before scoring.

### Compliance with `project.md`
- **§2.2, §2.7.3, §2.7.4, §3.4.3:** Direct realization of replication planning and routing.
- **INV-5 & INV-6:** Prefix ordering, exact offset preservation, and idempotent range re-transfers.

---

## ADR-0008: Partition Ownership, Epoch Fencing & Quarantine Store (`kamish-own`)

### Context
`project.md` §3.1.3, §3.1.4, §3.3.1, §3.4.4, INV-3, and INV-7 specify:
1. Failover promotion must be gated by:
   - Lease expiry + maximum clock drift margin (§4.7.2 R6).
   - Policy eligibility check via the single decision function (`PROMOTE`).
   - Quorum CAS update bumping partition epoch monotonically (`epoch + 1`).
2. Fencing stale writers: any write request arriving with epoch lower than partition epoch is rejected.
3. Reconnecting old owners: when a partitioned old leader heals and returns, its divergent/superseded log suffix is truncated and moved to a local `QuarantineStore`, which is strictly isolated and never replicated.

### Decision
Implement `kamish-own` containing `PartitionOwnerCoordinator` and `QuarantineStore` executing this exact safety logic.

### Compliance with `project.md`
- **§3.1.3, §3.1.4, §3.4.4:** Direct implementation of promotion, fencing, and quarantine.
- **INV-3 & INV-7:** Fencing guarantees split-brain immunity across all tested partition scenarios.

---

## ADR-0009: Key Residency and Sealed-Copy Mode (`kamish-seal`)

### Context
`project.md` §1.6 D2, §2.4, §3.4.5, §3.5.2, and §4.7.1 specify:
1. Destination mode `SealedOnly`: data is replicated as authenticated ciphertext without local keys.
2. Simulated Key Service (`RegionalKeyService`): keys remain exclusively resident in the class's home key region. Cross-border requests for key bytes are denied.

### Decision
Implement `kamish-seal` featuring `RegionalKeyService` and `SealEngine` with authenticated encryption (`SealedEnvelope`).

### Compliance with `project.md`
- **§1.6 D2 & §2.4:** Fulfills sovereign off-region durability without cleartext data egress.

---

## ADR-0010: Automated Invariant Checkers & Canary Scanner (`kamish-checkers`)

### Context
`project.md` §1.4.1 FR-10, §3.5.3, and §4.4 require test checkers for:
1. `OrderingChecker`: verifies dense monotonic offsets per partition (INV-1).
2. `EpochAuditor`: verifies monotonic epoch progression and absence of stale writes (INV-7).
3. `CanaryScanner`: scans raw payload across regions and catches synthetic canary leaks in unauthorized regions (INV-8).

### Decision
Implement `kamish-checkers` as a shared auditing suite used during test scenarios and CI runs.

### Compliance with `project.md`
- **§3.5.3 & §4.4:** Full automated validation for core invariants.
