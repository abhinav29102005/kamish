# KAMISH — REMAINING WORK BACKLOG

**Based on Inspected Revision:** `49752d0` (Branch: `main`)  
**Date:** 2026-11-01  
**Status:** Authoritative Post-Completion Backlog (Phase Three Handover)

---

## 1. Backlog Governance & Current Status

Following the completion of the One-Shot Remaining Implementation & Functional Completion campaign, all P0, P1, P2, and P3 engineering gaps have been fully implemented, integrated into daemon execution paths, and committed to `main`.

No implementation blockers or unintegrated core features remain open. The only open items are explicitly categorized as **Deferred Architecture Decisions**, which require external cloud infrastructure custody agreements or multi-node in-region clustering decisions.

---

## 2. Resolved Implementation Items (One-Shot Completion Campaign)

The following items from the post-reconnaissance backlog were completed and committed:

### ITEM-P1-01: Production Authenticated Encryption (AEAD) for Sealed Cross-Region Replication
- **Status:** **RESOLVED** in commit `85b293c` (`feat(security): integrate AEAD and key provider boundaries`).
- **Implementation:** Integrated standard IETF ChaCha20-Poly1305 and AES-256-GCM via `ring::aead` in `crates/kamish-seal`. Generates 96-bit CSPRNG nonces per record with AAD binding `(topic, partition, epoch, policy_version, key_id, key_version)`. Implemented `KeyProvider` trait, `DevKeyProvider` (isolated for offline development), and `ExternalKmsProvider` boundary. Wired into `BrokerServer::produce` with fail-closed behavior when external KMS is unconfigured.

### ITEM-P1-02: Autonomous Audit Witness Publisher Daemon
- **Status:** **RESOLVED** in commit `5e7cdf9` (`feat(audit): add durable witness publication`).
- **Implementation:** Implemented `WitnessPublisher` with `witness_publisher_journal.json` crash-safe persistence (temporary file `sync_all` and atomic rename) in `crates/kamish-audit/src/publisher.rs`. Added `WitnessDestination` trait, `UnconfiguredWitnessDestination`, and `FileWitnessDestination`. Spawned `spawn_witness_publisher_worker` in `cmd/kamish-broker/src/main.rs` running autonomous periodic background sweeps with graceful shutdown.

### ITEM-P2-01: Automated Segment Retention & Garbage Collection Worker
- **Status:** **RESOLVED** in commit `36819bd` (`feat(storage): add safe segment retention lifecycle`).
- **Implementation:** Added `base_offset_floor` tracking to `PartitionLog` in `crates/kamish-log/src/lib.rs` and implemented `reclaim_prefix_before_offset` with temporary file `sync_all` and atomic rename. Implemented `RetentionCoordinator` in `cmd/kamish-broker/src/retention.rs` calculating strict reclamation ceiling $\min(\text{consumer\_offsets}, \text{outbox\_watermarks})$ to guarantee unread/un-replicated records are never deleted. Spawned `spawn_retention_worker` in `cmd/kamish-broker/src/main.rs`.

### ITEM-P2-02: Dynamic Consumer Group Coordinator & Rebalancer
- **Status:** **RESOLVED** in commit `388d469` (`feat(consumers): implement consumer group coordination`) and `49752d0` (`feat(core): integrate gateway consumer group and retention endpoints`).
- **Implementation:** Implemented `ConsumerGroupCoordinator` in `cmd/kamish-broker/src/consumer_group.rs` supporting dynamic membership, session heartbeats, deterministic round-robin partition assignment, generation fencing on `commit_offset`, and crash-safe persistence to `consumer_groups.json`. Added wire commands `JoinGroup`, `HeartbeatGroup`, `LeaveGroup`, `ListGroups`, `DescribeGroup`, gateway REST endpoint `GET /api/consumer-groups`, and TypeScript SDK methods.

### ITEM-P3-01: Standardized Prometheus Metrics Exporter
- **Status:** **RESOLVED** in commit `97e99eb` (`feat(observability): expose broker and replication metrics`).
- **Implementation:** Implemented `BrokerMetricsRegistry` in `cmd/kamish-broker/src/metrics.rs` with lock-free atomic counters for produce, fetch, replication, consumer commits, outbox queue depths, quarantine counts, consumer groups, retention reclaimed bytes, witness checkpoints, and sealed replication. Exposes standard Prometheus text format 0.0.4 at `GET /metrics` in `cmd/kamish-gateway/src/lib.rs` (bypassing session auth).

### ITEM-P3-02: Automated Multi-Region Performance Benchmark Runner
- **Status:** **RESOLVED** in commit `e3a4c13` (`feat(benchmarks): add reproducible scenario orchestration`).
- **Implementation:** Created `benchmarks/schema.json` defining strict JSON schema for benchmark runs, topology, and results. Implemented `benchmarks/orchestrator.mjs` validating local binary and external container/tool prerequisites (Docker, Kafka 3.9.0, Pulsar 4.0.0, Toxiproxy). Emits versioned run manifests (`run-<system>-<timestamp>.json`) in `benchmarks/results/` with `status: "CONFIGURED_UNEXECUTED"` when live runs or prerequisites are absent, strictly avoiding fake performance data.

---

## 3. Deferred Work (Requires Architecture Decision or External Dependency)

The following items are deferred pending formal architecture decisions and external infrastructure agreements:

### ITEM-DEF-01: In-Region Multi-Node Replica Quorum Engine
- **Missing Behavior:** True `AckLevel::InRegionQuorum` across separate physical server nodes.
- **Why Deferred:** `BrokerServer` is currently designed as a high-performance single-process regional daemon managing local NVMe storage (`AckLevel::LeaderFsync`) and replicating across WAN to other regional brokers via outbox queues. Implementing in-region clustering would require either a full Raft consensus engine (e.g., `openraft` / `tikv/raft-rs`) or an active-active in-region gossip replication protocol across multiple broker daemon instances, fundamentally altering the broker's process model.
- **Prerequisite:** Formal architectural decision from maintainers on whether Kamish should include intra-region multi-node clustering or maintain single-node regional brokers with NVMe replication.

### ITEM-DEF-02: Cloud KMS Key Custody Integration for Sealed Replication
- **Missing Behavior:** Live integrations with AWS KMS, Google Cloud KMS, Azure Key Vault, or HashiCorp Vault for envelope key unwrapping.
- **Why Deferred:** Requires target cloud infrastructure credentials and an agreement on whether to support multi-cloud key federation or standard PKCS#11 / Vault interfaces. The cryptographic engine (`crates/kamish-seal`) and `ExternalKmsProvider` trait boundary are already implemented and fail closed when KMS is unconfigured.
- **Prerequisite:** Maintainer design decision on supported cloud key providers and credentials management.
