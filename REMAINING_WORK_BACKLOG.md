# KAMISH — REMAINING WORK BACKLOG

**Based on Inspected Revision:** `fb6341d` (Branch: `main`)  
**Date:** 2026-11-01  
**Status:** Authoritative Post-Gap-Closure Completion Backlog

---

## 1. Backlog Governance & Current Status

Following the completion of the In-Region Quorum Directive and the Final Functional Gap Closure & Integration Audit campaign, all in-scope functional backlog items across Kamish are implemented, integrated into daemon execution paths, and committed to `main`.

No implementation blockers or unintegrated core features remain open. The only open item is explicitly categorized as an **External Infrastructure Dependency**, which requires external cloud infrastructure custody agreements for live KMS unwrapping.

---

## 2. Resolved Implementation Items

The following items from the backlog have been completed, integrated into runtime daemon execution paths, and committed:

### ITEM-DEF-01 / DUR-02: In-Region Multi-Node Replica Quorum Engine & Synchronous Durability
- **Status:** **RESOLVED** across commits `d84313c`, `88398bf`, `3dcbd7f`, and `add2ff8`.
- **Implementation:** Implemented primary-backup multi-node quorum consensus in `crates/kamish-own/src/quorum.rs` and `cmd/kamish-broker/src/quorum_manager.rs`. Replicas enforce persistent term and epoch state (`consensus_state.json` via atomic rename), majority quorum sizing ($M = \lfloor V/2 \rfloor + 1$), concurrent TCP log replication, epoch and term fencing on followers, uncommitted suffix divergence quarantine into `QuarantineStore`, and majority commit watermark advancing. Follower commit safety guarantees leaders can never truncate committed history. Consumer fetch paths strictly enforce invariant INV-4 (capping readable offset at $\min(\text{high\_watermark}, \text{commit\_index})$) to prevent dirty reads. Exposed live status via broker wire protocol (`QuorumStatus`), CLI flags (`--node-id`, `--ack-level`), and gateway endpoint `GET /api/quorum`.

### ITEM-P1-01 / SEC-05: Production Authenticated Encryption (AEAD) & Key Custody Configuration
- **Status:** **RESOLVED** in commits `85b293c` and `b7cbeec`.
- **Implementation:** Integrated standard IETF ChaCha20-Poly1305 and AES-256-GCM via `ring::aead` in `crates/kamish-seal`. Generates 96-bit CSPRNG nonces via `OsRng` per record with AAD binding `(tenant, topic, partition, epoch, policy_version, key_id, key_version)`. Implemented `KeyProvider` trait, `DevKeyProvider` (isolated for offline development), and `ExternalKmsProvider` boundary. Added `kms: Option<ExternalKmsConfig>` to `BrokerConfig` in `cmd/kamish-broker/src/config.rs` and wired into `main.rs` daemon startup, failing closed when external KMS is unconfigured.

### ITEM-P1-02 / SEC-07: Autonomous Audit Witness Publisher Daemon
- **Status:** **RESOLVED** in commits `5e7cdf9` and `c9fe2da`.
- **Implementation:** Implemented `WitnessPublisher` with `witness_publisher_journal.json` crash-safe persistence (temporary file `sync_all` and atomic rename via `NamedTempFile::persist`) in `crates/kamish-audit/src/publisher.rs`. Added deterministic stable request IDs (`wit-req-<topic>-<partition>-<seq>-<hash>`) for idempotent delivery. Added `WitnessDestination` trait, `UnconfiguredWitnessDestination`, and `FileWitnessDestination`. Spawned `spawn_witness_publisher_worker` in `cmd/kamish-broker/src/main.rs` running autonomous periodic background sweeps with graceful shutdown.

### ITEM-P2-01 / LOG-06: Automated Segment Retention & Safe Storage Lifecycle
- **Status:** **RESOLVED** in commits `36819bd` and `fb6341d`.
- **Implementation:** Added `base_offset_floor` tracking to `PartitionLog` in `crates/kamish-log/src/lib.rs` and implemented `reclaim_prefix_before_offset` with temporary file `sync_all` and atomic rename, retaining the active segment tail to preserve segment metadata. Implemented `RetentionCoordinator` in `cmd/kamish-broker/src/retention.rs` calculating strict reclamation ceiling $\min(\text{high\_watermark}, \min(\text{quorum\_commit}), \min(\text{consumer\_offsets}), \min(\text{outbox\_watermarks}))$ to guarantee unread, uncommitted, or un-replicated records are never deleted. Spawned `spawn_retention_worker` in `cmd/kamish-broker/src/main.rs`. Added `QuarantineStore::prune_quarantine` in `crates/kamish-own/src/lib.rs` with explicit 100k bound.

### ITEM-P2-02 / CNS-04: Dynamic Consumer Group Coordinator & Partition Ownership Fencing
- **Status:** **RESOLVED** in commits `388d469`, `49752d0`, and `fb6341d`.
- **Implementation:** Implemented `ConsumerGroupCoordinator` in `cmd/kamish-broker/src/consumer_group.rs` supporting dynamic membership, session heartbeats, deterministic round-robin partition assignment, generation fencing, partition ownership validation on `commit_offset`, and crash-safe persistence to `consumer_groups.json`. Spawned background `spawn_consumer_group_sweeper_worker` in `main.rs` to automatically evict dead members and advance generation. Enforced tenant isolation on `JoinGroup`. Added wire commands, gateway REST endpoint `GET /api/consumer-groups`, and TypeScript SDK methods.

### ITEM-P3-01 / OPS-03: Standardized Prometheus Metrics Exporter
- **Status:** **RESOLVED** in commits `97e99eb`, `88398bf`, and `add2ff8`.
- **Implementation:** Implemented `BrokerMetricsRegistry` in `cmd/kamish-broker/src/metrics.rs` with lock-free atomic counters for produce, fetch, replication, quorum appends, quorum availability, partition commit watermarks, consumer commits, outbox queue depths, quarantine counts, consumer groups, retention reclaimed bytes, witness checkpoints, and sealed replication. Exposes standard Prometheus text format 0.0.4 at `GET /metrics` in `cmd/kamish-gateway/src/lib.rs` (bypassing session auth).

### ITEM-P3-02 / OPS-02: Automated Multi-Region Performance Benchmark Runner
- **Status:** **RESOLVED** in commits `e3a4c13` and `fb6341d`.
- **Implementation:** Created `benchmarks/schema.json` defining strict JSON schema for benchmark runs, topology, and results. Implemented `benchmarks/orchestrator.mjs` validating local binary and external container/tool prerequisites (Docker, Kafka 3.9.0, Pulsar 4.0.0, Toxiproxy). Supports dual ackLevels (`LeaderFsync`, `InRegionQuorum`) comparable to Kafka `acks=all` and Pulsar `ackQuorum=2`. Emits versioned run manifests in `benchmarks/results/` with `status: "CONFIGURED_UNEXECUTED"` when live runs or prerequisites are absent, strictly avoiding fake performance data.

---

## 3. Deferred Work (Requires External Infrastructure Dependency)

The following item is deferred pending external cloud infrastructure provisioning and custody agreements:

### ITEM-DEF-02: Cloud KMS Key Custody Integration for Sealed Replication
- **Missing Behavior:** Live integrations with AWS KMS, Google Cloud KMS, Azure Key Vault, or HashiCorp Vault for envelope key unwrapping.
- **Why Deferred:** Requires target cloud infrastructure credentials and an agreement on whether to support multi-cloud key federation or standard PKCS#11 / Vault interfaces. The cryptographic engine (`crates/kamish-seal`), `ExternalKmsProvider` trait boundary, and `BrokerConfig.kms` wiring are already implemented and fail closed when KMS is unconfigured.
- **Prerequisite:** Maintainer design decision on supported cloud key providers and credentials management.
