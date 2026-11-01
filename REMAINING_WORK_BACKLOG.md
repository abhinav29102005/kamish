# KAMISH — REMAINING WORK BACKLOG

**Based on Inspected Revision:** `6a5ef4a4802e613441cdc8a3ce5e225add236a2c` (Branch: `main`)  
**Date:** 2026-11-01  
**Status:** Authoritative Post-Audit Backlog (Replaces Stale Backlogs)

---

## 1. Backlog Governance & Prioritization Principles

This backlog reflects the genuine, confirmed engineering gaps identified during the static source reconnaissance and architectural reality audit. All previously resolved items (such as FN-02.1 truncation atomicity, FN-04 provenance watermarks, consumer offset fsync, and broker wire RBAC) have been purged.

Priority categories:
- **P0 — Critical correctness or security gaps:** Immediate risks of data corruption, silent data loss, split-brain, or security bypass.
- **P1 — Required core functionality:** High-value platform capabilities that are currently partially implemented, fail-closed, or missing required integration.
- **P2 — Operational completeness:** Autonomous management, background housekeeping, and consumer scalability.
- **P3 — Productization and release preparation:** Observability, packaging, and developer tooling.
- **Deferred:** Large architectural expansions requiring external infrastructure dependencies or formal design decisions.

---

## 2. Prioritized Backlog Items

### P0 — Critical Correctness or Security Gaps

*Note: After the completion of FN-02.1 (failover atomicity), FN-04 (provenance watermarks), Section 34 (wire RBAC and checkpoint fsync), no P0 critical correctness or security gaps exist within the active single-node `LeaderFsync` execution path.*

---

### P1 — Required Core Functionality

#### ITEM-P1-01: Production Authenticated Encryption (AEAD) for Sealed Cross-Region Replication
- **Exact Missing Behavior:** `crates/kamish-seal` contains a mock XOR-CTR transformation labeled as an educational prototype. In `cmd/kamish-broker/src/lib.rs`, topics configured with `ResidencyPolicy::SealedCiphertext` fail closed with `"sealed_replication_not_configured"`. Production sealed replication cannot execute.
- **Why It Matters:** Multi-region tenants with strict data residency boundaries (e.g. EU GDPR or HIPAA) cannot replicate encrypted envelopes across untrusted WAN links without exposing plaintext or having requests rejected.
- **Relevant Source Locations:**
  - `crates/kamish-seal/src/lib.rs` (cipher implementation)
  - `cmd/kamish-broker/src/lib.rs:412` (broker residency policy check)
  - `cmd/kamish-agent/src/lib.rs:245` (agent replication dispatch)
- **What Must Change:**
  1. Replace the mock XOR implementation with standard IETF ChaCha20-Poly1305 (or AES-256-GCM) using a crate such as `chacha20poly1305`.
  2. Implement an authenticated encryption envelope with a 96-bit random nonce and Additional Authenticated Data (AAD) binding topic name, partition index, epoch, and policy version.
  3. Define a Key Provider trait (`KeyProvider`) to obtain envelope encryption keys (DEK) wrapped by a Key Encryption Key (KEK).
  4. Update `BrokerServer::produce` to encrypt payload before staging to outbox when sealed residency is enabled.
- **Dependencies and Prerequisites:** Selection or mocking of a Key Custody provider (see ITEM-DEF-02).
- **Acceptance Condition:** A produce request for a sealed topic produces an encrypted payload on disk and wire with valid Poly1305 authentication tag; follower verifies tag without having access to plaintext payload; broker no longer returns `"sealed_replication_not_configured"`.
- **Classification:** Implementation work.
- **Reuse Opportunity:** Reuse `SealedEnvelope` wire framing and serialization in `crates/kamish-seal`.

---

#### ITEM-P1-02: Autonomous Audit Witness Publisher Daemon
- **Exact Missing Behavior:** Cryptographic hash chains (`crates/kamish-audit`) and Ed25519 checkpoint signing (`AuditWitness`) are fully implemented and tested. However, neither `BrokerServer` nor `kamish-gateway` runs an autonomous background worker that periodically pushes signed checkpoints to an independent external witness authority.
- **Why It Matters:** Without continuous off-site checkpoint commitment, an adversary who gains root access to the broker host could theoretically truncate the audit file, rewrite log events, and recompute the rolling SHA-256 digests undetected.
- **Relevant Source Locations:**
  - `crates/kamish-audit/src/witness.rs` (`AuditWitness`)
  - `cmd/kamish-broker/src/lib.rs` (broker daemon lifecycle)
  - `cmd/kamish-gateway/src/main.rs` (gateway daemon lifecycle)
- **What Must Change:**
  1. Add an `AuditWitnessPublisher` background task running on a configurable interval (e.g. every 60 seconds).
  2. The task inspects `AuditEventLog::latest_digest()`, signs the checkpoint with the broker's witness private key, and submits the signed checkpoint over HTTPS to the configured external witness endpoint (or S3/GCS bucket).
- **Dependencies and Prerequisites:** Configured external witness HTTPS endpoint or storage bucket credentials.
- **Acceptance Condition:** The broker periodically outputs signed checkpoints to an external destination; if the external witness fails, an alert metric is emitted without blocking produce traffic.
- **Classification:** Implementation work.
- **Reuse Opportunity:** Directly reuses `AuditWitness::sign_checkpoint` and `CheckpointPayload` from `crates/kamish-audit/src/witness.rs`.

---

### P2 — Operational Completeness

#### ITEM-P2-01: Automated Segment Retention & Garbage Collection Worker
- **Exact Missing Behavior:** `PartitionLog::truncate_to_offset` exists for failover divergence, and `delete_segments_before` exists for manual cleanup, but there is no scheduled background task that automatically sweeps and deletes log segments older than the topic's retention time or exceeding disk size quotas.
- **Why It Matters:** Under sustained production write workloads, partition directories will eventually exhaust all host disk space unless an operator runs external maintenance scripts.
- **Relevant Source Locations:**
  - `crates/kamish-log/src/lib.rs:610` (`delete_segments_before`)
  - `cmd/kamish-broker/src/lib.rs` (broker background tasks)
- **What Must Change:**
  1. Spawn a `LogCleanerWorker` in `BrokerServer::new` running on a configurable interval (e.g. every 5 minutes).
  2. For each active partition, compare segment creation timestamps and cumulative size against topic retention policy (`retention_ms`, `retention_bytes`).
  3. Safely delete sealed segments whose high watermark is strictly below the lowest active consumer group offset and replication outbox watermark.
- **Dependencies and Prerequisites:** None.
- **Acceptance Condition:** Sealed segment files older than `retention_ms` are deleted from disk; active log segments and un-replicated outbox ranges are never deleted.
- **Classification:** Implementation work.
- **Reuse Opportunity:** Reuses existing `PartitionLog` segment inspection methods.

---

#### ITEM-P2-02: Dynamic Consumer Group Coordinator & Rebalancer
- **Exact Missing Behavior:** `BrokerServer` provides atomic offset commit and fetch (`commit_offset`, `fetch_offset`), but does not implement dynamic group membership (JoinGroup, SyncGroup, Heartbeat) or automatic partition assignment. Consumers must be assigned fixed partition IDs statically.
- **Why It Matters:** Horizontal scaling of client consumers requires dynamic partition rebalancing when consumer instances join, crash, or leave the group.
- **Relevant Source Locations:**
  - `cmd/kamish-broker/src/lib.rs` (broker commands)
  - `crates/kamish-wire/src/lib.rs` (wire protocol frames)
  - `packages/kamish-client/src/` (SDK consumer)
- **What Must Change:**
  1. Add `JoinGroup`, `SyncGroup`, and `Heartbeat` protocol commands to `crates/kamish-wire`.
  2. Implement an in-memory `GroupCoordinator` in `BrokerServer` tracking member heartbeats and rebalancing partition assignments using a range or round-robin assignor.
- **Dependencies and Prerequisites:** Client SDK updates to handle group rebalance signals.
- **Acceptance Condition:** Starting a second consumer process automatically triggers partition rebalancing, splitting topic partitions between the active consumers.
- **Classification:** Implementation work.
- **Reuse Opportunity:** Reuses existing consumer offset persistence (`consumer_offsets.json`).

---

### P3 — Productization and Release Preparation

#### ITEM-P3-01: Standardized Prometheus / OpenTelemetry Metrics Exporter
- **Exact Missing Behavior:** The broker and agent log metrics to stdout and provide Server-Sent Events (SSE) via the gateway, but do not expose a standard `/metrics` HTTP endpoint for Prometheus scraping.
- **Why It Matters:** Production deployments require integration with standard observability stacks (Prometheus, Grafana, Datadog).
- **Relevant Source Locations:**
  - `cmd/kamish-broker/src/main.rs`
  - `cmd/kamish-agent/src/main.rs`
  - `cmd/kamish-gateway/src/main.rs`
- **What Must Change:**
  1. Integrate `metrics` and `metrics-exporter-prometheus` crates.
  2. Expose an HTTP listener on a configurable port (e.g. `:9090/metrics`) exporting latency histograms (p50, p99), produce rate, fetch rate, outbox byte queue depth, and circuit breaker states.
- **Dependencies and Prerequisites:** None.
- **Acceptance Condition:** `curl http://localhost:9090/metrics` returns standard Prometheus text exposition format containing all broker and agent health counters.
- **Classification:** Productization work.

---

#### ITEM-P3-02: Automated Multi-Region Performance Benchmark Runner
- **Exact Missing Behavior:** `benchmarks/baselines/` contains static configuration files for Kafka MirrorMaker 2 and Pulsar, but lacks an automated executable benchmark runner that tests throughput, replication lag, and failover latency under synthetic WAN conditions.
- **Why It Matters:** Performance claims cannot be defended in technical marketing or architectural evaluations without reproducible benchmark artifacts.
- **Relevant Source Locations:**
  - `benchmarks/baselines/`
  - `scripts/`
- **What Must Change:**
  1. Create a runnable benchmark orchestration script (`scripts/run-benchmarks.sh` / `.ps1`) that spins up 2 regional brokers with artificial WAN delay (e.g. 50ms RTT using Linux `tc` or toxiproxy).
  2. Drive sustained produce traffic and record latency percentiles (p50, p99, p99.9) and replication lag.
  3. Output structured JSON/CSV results to `benchmarks/results/`.
- **Dependencies and Prerequisites:** Toxiproxy or Linux network namespace support.
- **Acceptance Condition:** Benchmark script runs unattended and generates reproducible throughput and latency comparison graphs.
- **Classification:** Tooling and documentation work.

---

### Deferred Work (Requires Architecture Decision or External Dependency)

#### ITEM-DEF-01: In-Region Multi-Node Replica Quorum Engine
- **Missing Behavior:** True `AckLevel::InRegionQuorum` across separate physical server nodes.
- **Why Deferred:** `BrokerServer` is currently designed as a high-performance single-process regional daemon managing local NVMe storage and replicating across WAN to other regional brokers. Implementing in-region clustering would require either a full Raft consensus engine or an active-active in-region gossip replication protocol, fundamentally altering the broker's process model.
- **Prerequisite:** Formal architectural decision from maintainers on whether Kamish should include intra-region multi-node clustering or maintain single-node regional brokers with NVMe replication.

#### ITEM-DEF-02: Cloud KMS Key Custody Integration for Sealed Replication
- **Missing Behavior:** Live integrations with AWS KMS, Google Cloud KMS, Azure Key Vault, or HashiCorp Vault for envelope key unwrapping.
- **Why Deferred:** Requires target cloud infrastructure credentials and an agreement on whether to support multi-cloud key federation or standard PKCS#11 / Vault interfaces.
- **Prerequisite:** Maintainer design decision on supported cloud key providers.

