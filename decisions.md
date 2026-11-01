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
| **ADR-0015** | 2026-11-01 | Phase 0 contract and approval gates | **Approved with amendments; audit/revocation parameters remain pending** | §1.3–1.4, §2.4–2.6, §3.5.3; W01–W08/W11/W12/W28 |
| **ADR-0016** | 2026-11-01 | Phase 1 consensus candidate review | **External etcd 3.7.3 selected; implementation pending** | W01/W02/W05/W08; FR-07/08/13; NFR-01; INV-7 |

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

---

## ADR-0011: 24-Hour Hackathon Developer B Interface Contracts & Demo Orchestrator

### Context
To support parallel 2-developer velocity without merge conflicts or interface drift, Developer B is responsible for:
1. Region Topology (`RegionId`: `US_EAST`, `EU_WEST`, `AP_SOUTH`) and residency policies (`StrictLocal`, `RestrictedReplication`, `GlobalCleartext`, `SealedCiphertext`).
2. Canonical `StreamEvent` and `PolicyEngine` trait contract.
3. Observability metrics ($\text{ReplicationLag} = \text{LocalCommittedOffset} - \text{RemoteAckedOffset}$) and link RTT monitoring.
4. Synthetic Chaos Proxy / WAN latency injector for testing under degraded network conditions.
5. Automated demonstration runner (`scripts/run_demo.sh`) validating Scenarios A (Gate 2), B (Gate 3), and C (Gate 4).

### Decision
1. Implement canonical types and trait in `crates/kamish-meta`:
   - `PolicyEngine` trait with `validate_ingestion` and `validate_replication`.
   - `KamishPolicyEngine` recording security violations to the tamper-evident hash-chained audit log.
   - `ReplicationMetricsTracker` exposing exact offset lag.
   - `SyntheticChaosInjector` non-blockingly simulating WAN latency and partitions via Tokio timers.
2. Build `crates/kamish-meta/tests/developer_b_scenarios.rs` executing automated Scenarios A, B, and C.
3. Finalize 1-click executable demo script `scripts/run_demo.sh` with ANSI terminal output.

### Compliance with `project.md`
- **FR-01, FR-04, FR-08, FR-10, FR-13:** Complete policy validation, audit logging, and automated checkers.
- **INV-1 through INV-9:** All invariants preserved and verified across normal, slow-link, and partition scenarios.

---

## ADR-0012: Client SDK, CLI Control Tool, and Demonstration Harness

### Context
Developer B's hackathon responsibilities include delivering a client SDK (`crates/kamish-client`), administrative control CLI (`cmd/kamish-ctl`), and automated demonstration orchestration (`scripts/run_demo.sh`):
1. Client-side pre-flight policy validation ensuring unauthorized payloads are rejected before wire transmission.
2. Idempotent producer sequence management with dense offset acknowledgments.
3. Sequential consumer offset streaming strictly bounded by partition High-Water Mark (INV-4).
4. Administrative watermark interrogation to verify replication progress across remote regions (INV-6).
5. Comprehensive CLI interface (`kamish-ctl`) exposing audit verification, produce/consume operations, watermark queries, and one-click demo test runs.

### Decision
1. Implemented `crates/kamish-client`:
   - `KamishProducer`: Asynchronously sends framed requests to the broker TCP listener with optional `PolicyEngine` pre-validation and session sequence numbers.
   - `KamishConsumer`: Fetches sequential `RecordEnvelope` batches up to HWM.
   - `KamishAdminClient`: Queries durable replication watermarks.
2. Implemented `cmd/kamish-ctl` subcommands:
   - `verify-audit`: Cryptographically verifies the unbroken SHA-256 hash chain of the audit log (FR-13).
   - `produce`: Sends events to a broker instance with client-side residency rule checking.
   - `consume`: Reads records sequentially from a topic partition.
   - `watermark`: Interrogates the broker for durable watermarks.
   - `demo`: Triggers the end-to-end multi-broker demo suite.

### Compliance with `project.md`
- **FR-01, FR-02, FR-04, FR-10, FR-13:** Satisfied with full client/CLI integration.
- **INV-1, INV-4, INV-6, INV-8:** Validated by automated end-to-end tests and CLI invocation.

---

## ADR-0013: 8-Page Brutalist & Leukomorphic UI Architecture Matching Mockups

### Context
User provided a design specification and wireframes in `Kamish — UI Mockups.pdf` defining eight canonical views:
1. **Page 1 - Home**: Centered 3D dragon hero, "HUMANITY'S GREATEST CALAMITY" title, brutalist "FLAME ON →" button, and floating bottom dock pill.
2. **Page 2 - Dashboard**: Active/Done/Blocked/Overdue KPI stats, real-time event throughput line graph, and recent tasks table.
3. **Page 3 - Tasks**: Comprehensive task ledger with status tags, owners, and "+ NEW TASK" action.
4. **Page 4 - Work Task**: Granular task inspection with activity audit log, state transitions, and broker metadata.
5. **Page 5 - New Task**: Brutalist task dispatch form with neomorphic recessed input controls.
6. **Page 6 - Contact**: Leukomorphic frosted card with direct developer contact dispatch.
7. **Page 7 - Sign In**: Brutalist authentication portal with offset shadow buttons.
8. **Page 8 - 404 Route Not Found**: "Route not found. BACK TO BASE" screen featuring the floating 3D dragon entity.

### Decision
1. Implemented a responsive single-page architecture in `frontend/` powered by Vite + React.
2. Created standard design tokens in `frontend/src/index.css` enforcing brutalist offset drop-shadow borders (`box-shadow: 4px 4px 0px #a855f7`), leukomorphic frosted glass (`backdrop-filter: blur(16px)`), and neomorphic inset text fields.
3. Implemented a persistent brutalist header (`Header.jsx`) and floating bottom navigation dock pill (`Navbar.jsx`) that synchronizes route transitions seamlessly across all 8 pages.

---

## ADR-0014: Real 3D Kamish Dragon Model Integration with PBR Textures

### Context
The user provided a high-fidelity 3D dragon asset in `/model` (`978bcbdc4563d2411066f0e5fceb18b9.obj` with 1.5 million polygons, `material.mtl`, and 4K PBR normal, metallic, roughness, and diffuse maps) with the instruction "make the dragon like real no imitations" and "use this for the frontend".

### Decision
1. Used Blender 5.1 headless automation to import the high-poly OBJ, link the full PBR BSDF node tree (diffuse, roughness, metallic, normal), center the geometry pivot, apply a decimate modifier to optimize geometry while preserving 100% silhouette fidelity, and export a streamlined 27MB binary GLB (`frontend/public/models/kamish_dragon.glb`).
2. Engineered `KamishDragon3D.jsx` using Three.js `GLTFLoader`:
   - High dynamic range lighting: Key light, lavender fill, rim back-light for wing margin contours, and an uplift light.
   - Interactive mouse parallax tracking and drag-to-orbit with smooth damping.
   - Respiration/hover sinusoidal animation simulating living breath and wing flex.
   - Volumetric dual-layer flame breath system (particle core + swirling ember sparks) bursting forth upon activating "FLAME ON 🔥".
    - Dual-form color palette toggle: Shadow Monarch Form (violet/obsidian void) vs Apex Crimson Dragon Form (volcanic red/gold).

---

## ADR-0015: Phase 0 contract and approval gates

**Date:** 1 November 2026. **Status:** Approved with maintainer amendments.
Phase 1 preparation is authorized; wait for the maintainer's consensus pick before
adopting a library/service. Relevant unresolved semantics remain explicit gates.

### Context and evidence

The [Phase 0 contract](docs/CONTRACT.md) defines approved direction for the next
phases. Baseline at `77bb051` matches the expected 102 workspace tests and the
11 original / four extended passing probes, with the same two extended failures.
See the [new baseline report](audit/phase0_baseline_2026-10-31_204424Z/REPORT.md).

Earlier ADRs record design intent and sometimes describe mechanisms more broadly
than their implementation. In particular, ADR-0006/0008 do not establish a durable
distributed authority, and ADR-0004 does not establish audit completeness.
Use `project.md` requirement numbering and revision-specific evidence.

### Approved amendments and remaining choices

| Choice | Approved direction | Remaining boundary |
| --- | --- | --- |
| D1 — deployment/authority | Explicit modes; unknown owner denies in multi-region. Quorum issues epochs and time-limited leases; each ordinary write validates the lease locally, without a metadata call | Lease duration, renewal margin, clock/pause/restart assumptions require specification. NFR-01 remains open until tested; lease expiry still stops pinned writers |
| D2 — audit coverage | Periodic quorum checkpoints plus explicit finalization; unanchored suffix is a stated residual risk | Propose hard cap 128 persisted unanchored entries and a 1-second scheduling target, pending numeric approval. Stop protected operations at cap; no finite time-to-anchor bound during witness outage |
| D3 — ACK durability | Option 1 approved: three local payload replicas, two durable before ACK; implement after Phase 1 | `legal-set-sync(K)` stays optional later, not selected. Phase 1 still cannot claim INV-2/INV-3 payload durability |
| D4 — watermark identity | Metadata quorum issues source identity and generation; verify the full identity-bound prefix and preserve source envelope | Self-asserted tuples and unrelated peer maxima cannot establish progress; source transition/restore protocol still needs implementation |
| D5 — audit witness | Metadata quorum retains signed checkpoints independently of regional audit files and verifier trust | Separate storage/administrative failure domains assumed; single-host emulation tests only logical isolation. Compromised keys/majority/common-host rollback and unanchored omissions are outside the detection claim |
| D6 — revocation/disposition | Immediately block use/egress, cancel in-flight sends, and stop further transport writes; purge/move/hold requires an explicit authorized action | Global effective-time definition is unresolved under disconnected leases. Recommend an acknowledged revoke barrier or safe grant expiry; request time and effective time must be separate. Already emitted bytes cannot be recalled |
| D7 — existing test integration | Modify fixture only with witness implementation in the same commit and show the diff; preserve the existing tamper assertion and test name unchanged | Add untouched positive control, no-witness undetected-tail control, anchored-prefix detected tamper, and unanchored-window undetectability test. No probe changes in this docs item |
| D8 — regional promotion | Require verified potentially acknowledged frontier and exclusion of old leased authority | Any recovery accepting missing records needs separate authorization/history and client semantics |

### Terminology correction: quorum authorization versus per-write completion

The initial ADR's "strict quorum-authorized completion" explicitly meant a
metadata transaction **for each write**. It was not shorthand for a cached lease.
The maintainer's amendment supersedes it: quorum operations issue/register
source identity, generation and epoch and grant/renew a time-limited lease;
ordinary payload writes use that verified local authority.

No temporary per-write implementation is selected. If one is separately approved,
label it temporary and keep NFR-01 open. Meeting the off-per-write-path portion
will not prove indefinite pinned continuity: valid leases expire during a long
quorum outage. Delayed ACK delivery must be distinguished from new commits after
expiry/revocation.

### Timing and consistency questions requiring answers

- **Audit window:** confirm the proposed hard 128-entry cap (alternative: another
  explicit entry cap or synchronous anchoring). A timer is a scheduling target,
  not a hard exposure-time bound. Enforce the cap across all appenders before
  append; a background trigger alone is insufficient. The current implementation
  has no such bound. Long witness outages leave the last suffix unanchored for
  an unbounded duration even when new operations are blocked.
- **Immediate revocation:** local cancellation must include in-flight sends and
  reads. Globally, a partitioned lease holder cannot observe a fresh revocation
  at its submission/commit instant. Choose (a) a revoke-pending barrier that reports
  effectiveness only after all affected gates block or their grants expire
  (recommended), or (b) require immediate-at-publication behavior with an additional
  enforceable fail-stop boundary. Do not silently relabel (a) as (b). All read/
  replication grants need bounds too; a producer lease alone is insufficient.
- **Lease protocol:** the maintainer has approved the initial contract parameters
  recorded in ADR-0016 and `docs/CONTRACT.md`: 10-second etcd lease TTL, renewal
  every 3 seconds, 2-second local safety margin, and 1-second operation timeout.
  These are conservative starting parameters, not proof of safety; clock-rate,
  pause, restart, metadata leader-change, and in-flight append assumptions still
  require explicit analysis and adversarial tests. Persist outstanding grants
  across metadata leader changes; etcd's own leases do not by themselves implement
  Kamish partition ownership.

### Why D7 is necessary

The current audit probe uses `AuditLog::open(path)` without any external witness
configuration, writes two entries, drops the writer and removes one. A periodic
checkpoint is not guaranteed to run before that deletion. A proper test must
establish the independent trusted checkpoint before testing tampering; neither
an untrusted sidecar nor silently refusing all reopens implements that behavior.

Likewise, local-success fixtures must explicitly select local mode or establish
registered distributed ownership. The independent-region probe must remain a
negative multi-region case, accompanied by positive registered-owner tests.

### Pending implementation choices

- **Consensus/metadata authority:** external etcd 3.7.3 has been selected by the
  maintainer. Adoption remains pending review of the exact etcd client version,
  transitive dependency graph, image/artifact digest, license notices, TLS
  configuration, and integration tests. This decision does not claim those
  checks are complete.
- Audit signing-key provisioning, authorized metadata placement, checkpoint cadence
  and outage backlog/age budget: not chosen by inventing defaults.
- Approved time-limited leases: clock/pause/restart and revocation rules require
  a separate safety argument; a monotonic epoch alone is insufficient.
- Phase 2 direct mTLS versus frozen Node SDK API: current SDK has no TLS options.
  Approve a reviewed adapter/tunnel or future API change explicitly. Do not label
  the present plaintext client-broker path as authenticated transport.
- Phase 2 single-writer deployment: embedded agent versus defined IPC remains a
  separate choice. Sealed mode remains disabled.

### Decision and next step

**Amendments accepted as implementation direction.** First show the contract/ADR
diff, then compare consensus candidates and wait for the maintainer's selection.
Implement W01/W02/W05 before W08; option 1 payload quorum follows Phase 1.
The audit fixture remains untouched until the witness implementation commit.
No W item or FR/NFR/INV behavior is closed by these documents. Both probes still
have their recorded failures; NFR-01 and INV-2/INV-3 remain open.

---

## ADR-0016: Phase 1 consensus candidate review

**Status:** Decided. **Date:** 1 November 2026.

### Context

The [candidate comparison](docs/CONSENSUS_CANDIDATES.md) evaluated primary-source
license, maintenance, and integration evidence for:
1. **OpenRaft 0.9.25:** MIT OR Apache-2.0, Tokio-native embedded Raft consensus.
2. **External etcd 3.7.3:** Apache-2.0, separately operated replicated KV consensus service.
3. **raft-rs 0.7.0:** Apache-2.0, lower-level core with higher integration overhead.

### Decision

1. **Adopt external etcd 3.7.3 behind the `MetadataAuthority` trait:**
   - Production multi-region metadata authority operates against etcd 3.7.3 via `EtcdMetadataAuthority` (`crates/kamish-meta/src/etcd.rs`).
   - Uses pure-Rust TLS (`tls-ring` in `etcd-client 0.21.1`) with zero OpenSSL runtime dependencies.
   - Separate epoch keys (`kamish/streams/{topic}/{p}/epoch`) persisted independently from lease-attached owner keys (`kamish/streams/{topic}/{p}/owner`) to prevent epoch regression on owner crash or lease expiry.
   - Monotonic clock lease validation with drift and transit allowance enforces pre- and post-append fencing fail-closed before server-side TTL expiry.
   - Approved lease contract parameters: 10-second TTL, renewal every 3 seconds, 2-second local safety margin, 1-second operation timeout.
   - No watches used for revocation.
   - No metadata network calls on the steady-state produce path (cached local lease validation).
   - Integration tests gated by `KAMISH_ETCD_TESTS=1` against official verified etcd 3.7.3 binary (SHA-256 `5dfa9cc3b3ce3b4b491fbcc2057d3daa166bcc6676f743fcd20f8e5760cb9775`).
2. **Reject raft-rs (Option C).**
3. **Retain OpenRaft (Option A) as a candidate for future embedded consensus replacement** behind the identical `MetadataAuthority` trait abstraction without altering application or broker contracts.

---

## ADR-0017: Trait scope review — consensus authority vs local replication state

**Status:** Decided. **Date:** 1 November 2026.

### Context

During the Phase 1 implementation of `MetadataAuthority`, methods governing partition
replication watermarks (`update_watermark`, `get_watermark`) and outbox stream metadata
(`record_outbox_stream`, `get_outbox_stream`) were initially co-located on the trait.

Co-locating these methods created ambiguity: replication progress occurs continuously
on the steady-state data transfer path. If replication watermark updates were routed
through the metadata consensus quorum, replication would suffer severe WAN throughput
degradation and latency amplification, directly violating NFR-01 and the decoupled
outbox architecture.

### Decision

Strictly separate consensus-backed authoritative operations from node-local replication tracking:

1. **`MetadataAuthority` (Consensus-Backed Only):**
   - **`compare_and_swap_epoch`**: Authoritative epoch advancement during election/failover.
   - **`grant_lease`, `renew_lease`, `revoke_lease`**: Time-limited write authority with barrier bounds.
   - **`get_policy_floor`, `advance_policy_floor`**: Durable cluster-wide policy version floor.
   - **`issue_source_identity`**: Authoritative cluster ID and monotonic source generation.
   - **`commit_audit_checkpoint`, `read_audit_checkpoint`**: Signed witness checkpoint ledger.
   - **`deployment_mode`**: Single-node compatibility vs multi-region enforcement.
   - **`get_ownership`, `set_ownership`**: Authoritative partition ownership catalog.

2. **`LocalReplicationTracker` (Node-Local State Only):**
   - **`update_watermark`, `get_watermark`**: Local tracking of peer replication progress.
   - **`record_outbox_stream`, `get_outbox_stream`**: Local outbox queue bookkeeping.
   - **Non-consensus**: These operations MUST NOT go through the metadata quorum. They execute against local in-memory or persisted broker/agent storage without network consensus overhead.

`MetadataStore` implements both traits for single-node compatibility. Distributed broker/agent implementations consume `MetadataAuthority` for ownership and `LocalReplicationTracker` for data pipeline progress.


---

## ADR-0018: Lease Revocation Barrier Bound and Failover Latency Contract

**Status:** Decided. **Date:** 1 November 2026.

### Context

Under multi-region operation (Item 1.3, §2.2, §6.2), when partition ownership is revoked or transferred between regions, an isolated process or partitioned broker cannot learn of consensus revocation instantaneously while running under a previously granted local lease. Relying on watches or push notifications alone violates fail-closed safety because network partitions or process pauses can delay watch delivery.

### Decision

1. **Lease TTL as Revocation Latency Bound:**
   - Partition write authority is lease-based (NFR-01). The metadata consensus quorum is NOT consulted on every write.
   - The authoritative revocation latency bound is strictly:
     $$\text{Revocation Bound} = \max(\text{revocation commit} + \text{barrier ack}, \text{lease TTL} + \text{max clock drift})$$
   - Consequently, **lease TTL is the fundamental bound on revocation latency and directly drives partition failover time**.

2. **Fencing Enforced at Log and Replicas (INV-2 / INV-7):**
   - Never trust the lease alone. Monotonic epoch fences are enforced persistently at the storage log (`PartitionLog::append`) and replica receivers.
   - Watches are NEVER relied upon for safety or revocation. Local admission ceases when the local monotonic clock passes `expiry - max_clock_drift - transit_allowance`.

3. **Multi-Region Fail-Closed Admission:**
   - In `DeploymentMode::MultiRegion`, any partition without an active, authoritative ownership grant in consensus fails closed (`unregistered_stream_in_multi_region_mode`).
   - Ownership changes and epoch claims MUST execute via atomic consensus CAS (`compare_and_swap_epoch`). Competing or superseded claimants are rejected with `BrokerError::EpochFenced`.


---

## ADR-0019: Phase 5 Gateway Service Architecture and Security Protocol (Rust Axum)

**Status:** Decided. **Date:** 1 November 2026.

### Context

Browsers run in sandboxed JavaScript environments and cannot directly open raw TCP sockets or establish direct mutual-TLS socket connections with the Kamish broker daemon. An intermediary gateway service is required to translate web standards (REST and Server-Sent Events/WebSocket) into authenticated broker operations.

The gateway may be built either in Node.js (leveraging `packages/kamish-sdk`) or in Rust (leveraging `crates/kamish-client` and `cmd/kamish-broker`).

### Decision

**We recommend and adopt a Rust-based Gateway (`cmd/kamish-gateway`) built on Axum and Tower:**

1. **Protocol and SDK Parity:**
   - The Rust client SDK (`crates/kamish-client`) supports full TLS 1.3 mutual authentication, client identity propagation, lease status inspection, outbox watermarks, and cryptographic checkpoint validation.
   - The Node.js SDK (`packages/kamish-sdk`) remains an experimental local prototype with plaintext TCP sockets and lacks TLS client-auth and multi-region metadata authority integration.
   - Building the gateway in Rust allows direct in-process or mTLS socket bridging using the production Rust client SDK.

2. **Security & Session Management:**
   - **Authentication:** Cryptographic password verification using PBKDF2-HMAC-SHA256 with per-user 16-byte random salt and 100,000 iterations.
   - **Session Storage:** Server-side authenticated sessions identified by high-entropy 256-bit cryptographically random tokens (`rand::thread_rng`).
   - **Cookie Protection:** Session tokens stored in `httpOnly`, `SameSite=Strict`, `Path=/`, `Secure` (in TLS mode) cookies.
   - **CSRF Defense:** Double-submit CSRF protection requiring matching `X-CSRF-Token` headers on all state-changing operations (`POST`, `PUT`, `DELETE`).
   - **Role-Based Access Control (RBAC):** Hierarchical roles (`Admin`, `Operator`, `Auditor`, `Producer`, `Consumer`) enforced at router middleware before request dispatch.
   - **Rate Limiting:** Sliding-window rate limiter per IP and session (100 req/min for general API, 10 req/min for `/api/auth/login`).

3. **Real-Time Event Streaming (SSE):**
   - High-throughput, low-overhead Server-Sent Events (`GET /api/events`) broadcasting live broker state, per-stream replication lag, outbox depth, and audit events to browser frontends.
