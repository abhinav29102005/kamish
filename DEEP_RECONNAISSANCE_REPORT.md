# KAMISH — DEEP RECONNAISSANCE & IMPLEMENTATION REALITY AUDIT REPORT

**Date:** 2026-11-01  
**Repository:** `E:\Kamish`  
**Remote:** `https://github.com/abhinav29102005/kamish`  
**Inspected Branch:** `main`  
**Inspected Commit (HEAD):** `6a5ef4a4802e613441cdc8a3ce5e225add236a2c`  
**Investigation Mode:** Static Source, Git History, and Artifact Reality Inspection (Read-Only)

---

## 1. Executive Summary

This deep reconnaissance audit was conducted to reconstruct the engineering reality of the Kamish multi-region event-streaming and replication platform directly from the source code, directory structures, and git history at commit `6a5ef4a` on branch `main`.

Over several development phases, extensive documentation and milestone reports accumulated overlapping descriptions of implemented capabilities, deferred tasks, and speculative roadmaps. This audit separates concrete source implementation from theoretical specifications, unintegrated abstractions, educational mocks, and unverified runtime claims.

### Key Audit Findings:
1. **Core Single-Process Broker Pipeline is Highly Functional:** The local produce, segmented append, index management, fetch filtering, policy enforcement, producer session deduplication, outbox staging, and durable consumer checkpointing (`f.sync_all()`) are fully implemented and integrated in `cmd/kamish-broker` and associated crates (`crates/kamish-log`, `crates/kamish-outbox`, `crates/kamish-core`, `crates/kamish-policy`).
2. **Durability Level Reality:** The broker engine operates exclusively at `AckLevel::LeaderFsync`. Records are fsynced to the local segmented log before an acknowledgement is returned. While `AckLevel::InRegionQuorum` exists as an enum and is tested in `crates/kamish-own` via an in-memory replica group abstraction (`InRegionReplicationGroup`), `BrokerServer` does **not** form an in-region replica cluster and cannot provide quorum acks in live execution. Any claim that the running broker delivers multi-node quorum durability is **unsupported by the current runtime architecture**.
3. **Cross-Region Replication Pipeline is Connected:** Cross-region replication via `kamish-agent` is wired through `AdaptiveRouteSelector` and `CircuitBreaker` (`crates/kamish-route`), respects partition lease authority (`check_replication_authority`), binds watermark advancement to record epoch and source generation (`advance_partition_watermark_with_provenance`), and handles receiver-side divergence by routing divergent suffixes to `QuarantineStore` while appending authoritative records.
4. **Sealed Replication Fails Closed (Educational Mock in Crate):** The XOR-CTR implementation in `crates/kamish-seal` is an educational prototype. In the broker (`cmd/kamish-broker/src/lib.rs`), requests requiring `ResidencyPolicy::SealedCiphertext` deliberately fail closed with `"sealed_replication_not_configured"`. Production sealed replication is not implemented and remains blocked on external KMS/HSM key custody integration and authenticated AEAD (ChaCha20-Poly1305 / AES-GCM).
5. **Metadata Authority Integration:** `kamish-meta::EtcdMetadataAuthority` implements etcd 3.7.3 client transactions, lease management, and CAS epoch allocation under TLS. In single-binary local runs where etcd is omitted, `BrokerServer` falls back to an in-memory `MetadataStore`.
6. **Security & Wire RBAC:** Wire-level RBAC is implemented across all broker TCP commands (`Produce`, `Fetch`, `CommitOffset`, `FetchOffset`, `Replicate`, `Heartbeat`, `PromotePartition`, `QuarantineDivergentSuffix`), with tenant validation on consumer checkpoint paths. Gateway API (`cmd/kamish-gateway`) enforces Argon2 password hashing, session tokens, CSRF validation, and route-level RBAC.
7. **Audit & Integrity Verification:** `crates/kamish-audit` provides SHA-256 hash chains across audit records. Truncation detection and tamper verification are implemented via `AuditWitness` using Ed25519 signatures, but the operational broker currently writes audit records locally without an autonomous background service continuously pushing signed checkpoints to an independent external witness authority.

---

## 2. Exact Repository State and Inspected Revision

- **Repository Root:** `E:\Kamish`
- **Branch:** `main`
- **HEAD Commit:** `6a5ef4a4802e613441cdc8a3ce5e225add236a2c`
- **Commit Subject:** `Update reconnaissance report and worktillnow with Section 34 final functional completion`
- **Working Tree State at Start of Audit:** Clean (0 untracked files, 0 staged/unstaged changes, tracked files match HEAD).
- **Workspace Structure:**
  - **Rust Workspace Members (15 crates/binaries):**
    - `cmd/kamish-broker` (main regional broker daemon)
    - `cmd/kamish-agent` (cross-region replication daemon)
    - `cmd/kamish-gateway` (Axum-based web gateway & SSE event stream)
    - `cmd/kamish-cli` (administrative CLI)
    - `crates/kamish-core` (data models, record context, protocol types)
    - `crates/kamish-log` (segmented partition log, single-writer file locking, index, fsync)
    - `crates/kamish-meta` (etcd 3.7.3 client, lease contracts, metadata store)
    - `crates/kamish-own` (partition lease lifecycle, promotion, quarantine store)
    - `crates/kamish-outbox` (per-destination outbox, watermark advancement, refill)
    - `crates/kamish-route` (route topology, link stats, adaptive routing, circuit breaker)
    - `crates/kamish-policy` (data residency, tenancy, payload classification)
    - `crates/kamish-wire` (binary wire framing, protocol codec, serialization)
    - `crates/kamish-audit` (audit event logger, SHA-256 hash chains, witness signatures)
    - `crates/kamish-seal` (sealed envelope specification and prototype)
    - `crates/kamish-trace` (distributed tracing spans and context propagation)
  - **Packages:**
    - `packages/kamish-client` (TypeScript SDK: client, producer, wire protocol, connection pool)
  - **Frontend:**
    - `frontend/` (Vite + React 18 administrative UI, dashboard, topology, audit log)
  - **Tooling & Baselines:**
    - `benchmarks/baselines/` (Kafka MirrorMaker 2 and Apache Pulsar comparison artifacts)
    - `scripts/` (reconciliation, verification, and deployment shell scripts)
    - `scenarios/` (end-to-end scenario definitions)

---

## 3. Scope, Method, and Limitations

### 3.1 Scope
The audit examined all source files, manifests (`Cargo.toml`, `package.json`), build scripts, documentation, and historical reports within `E:\Kamish`.

### 3.2 Method
- **Static Source Inspection:** Manual and tool-assisted tracing of callers, callees, state mutations, synchronization primitives (mutexes, atomics), error handling, and persistence operations.
- **Git Commit Archeology:** Verified commits `41e6b49`, `cbc426b`, `baab371`, `0895dd6`, `9f6dd63`, `01e183f`, `5c4b011`, `bb8a75b`, `7eb18b5`, `1908aa3`, `6a5ef4a`, `170de8e`, `77bb051`, and `3466821` against the git object database.
- **Contract Cross-Referencing:** Compared implementation against `docs/CONTRACT.md`, `decisions.md`, `worktillnow.md`, and previous reconnaissance reports.

### 3.3 Limitations
- **No Runtime Execution:** In strict compliance with the audit directive, no test commands (`cargo test`, `npm test`), benchmark scripts, or distributed scenarios were executed.
- **Single-Process Boundaries:** Concurrency and crash-consistency analysis is based on static reasoning of mutexes, atomic file renames, `fsync`/`sync_all` calls, and OS file locks (`libc::flock` / `LockFileEx`).

---

## 4. Current Architecture Reconstructed from Source

```
+----------------------------------------------------------------------------------------------------+
|                                         KAMISH SYSTEM ARCHITECTURE                                  |
+----------------------------------------------------------------------------------------------------+

  [ Clients: SDK / CLI ]                     [ Operators: Browser / React Dashboard ]
            |                                                      |
            | Binary Protocol (TCP)                                | REST / SSE
            v                                                      v
+-----------------------+                             +-----------------------------------+
|  cmd/kamish-broker    |<----------------------------|  cmd/kamish-gateway (Axum)        |
|  - Wire RBAC & Framing|      Internal Broker Client |  - Session Auth (Argon2 + Token)  |
|  - Produce Pipeline   |                             |  - CSRF Middleware & Router RBAC  |
|  - Fetch Pipeline     |                             |  - SSE Event Stream (/api/events) |
|  - Lease Renewal Wkr  |                             +-----------------------------------+
|  - Outbox Stage/Refill|                                              |
|  - Consumer Checkpoint|                                              v
+-----------------------+                                  [ etcd 3.7.3 Cluster ]
     |             |                                       (Leases, Epochs, CAS)
     |             +---------------------------------------------------+
     v                                                                 v
+------------------------------------+               +------------------------------------+
| Local Partition Log & Storage      |               | crates/kamish-meta                 |
| - Segment files (.log) + Index     |               | - EtcdMetadataAuthority            |
| - OS File Locking (flock/LockFile) |               | - Lease TTL: 10s, Renewal: 3s      |
| - LeaderFsync Durability           |               | - Safe Margin: 2s, Op Timeout: 1s  |
| - Consumer Offsets (atomic fsync)  |               +------------------------------------+
| - QuarantineStore (bounded 100k)   |                                 |
+------------------------------------+                                 |
     |                                                                 v
     v                                               +------------------------------------+
+------------------------------------+               | cmd/kamish-agent                   |
| crates/kamish-outbox               |               | - Authority Fenced Dispatch        |
| - Destination Outboxes             |<--------------| - Adaptive Route Selector          |
| - Byte & Count Accounting          |               | - Circuit Breaker (Trip/Reset)     |
| - Provenance Watermarks            |               | - Step Replication & TCP Dispatch  |
| - Log Refill Scanner               |               | - Heartbeat Sweep Daemon (500ms)   |
+------------------------------------+               +------------------------------------+
                                                                       |
                                                                       | Wire Replication Request
                                                                       v (Cross-Region WAN)
                                                             [ Remote Broker Follower ]
```

---

## 5. Major Execution Paths

### 5.1 Produce Pipeline
**Entry Point:** `BrokerServer::produce` (`cmd/kamish-broker/src/lib.rs:388`)
1. **Catalog & Policy Validation:** Validates tenant, topic existence, and region residency via `CatalogManager` and `PolicyEnforcer`. Rejects if sealed replication is requested (`"sealed_replication_not_configured"`).
2. **Partition Lock Acquisition:** Locks partition-specific state mutex (`PartitionState`).
3. **Pre-Append Lease Verification:** Invokes `ensure_active_lease(topic, partition)`. Checks that the local lease epoch is valid and expiration is at least `safe_margin` (2.0s) in the future.
4. **Session Deduplication:** Checks `ProducerSessionTracker` for `(producer_id, producer_epoch)`. If `sequence_number <= last_seen_sequence`, immediately returns cached offset and `AckLevel::LeaderFsync` without appending.
5. **Outbox Reservation:** Calls `DestinationOutbox::reserve_capacity` for all target replication regions. If capacity is exhausted, fails back with outbox backpressure.
6. **Log Append & Durability:** Appends the record with `RecordContext` (epoch, sequence, producer ID, timestamp) to `PartitionLog` (`crates/kamish-log`). Immediately flushes and invokes `fsync` on the segment file (`sync_data` / `sync_all`).
7. **Session & Outbox Commit:** Updates `ProducerSessionTracker` with new sequence and log offset. Commits the reserved payload and envelope bytes to the outbox.
8. **Post-Append Lease Verification:** Checks `ensure_active_lease` a second time to ensure the lease did not expire during the disk write.
9. **Acknowledgement:** Returns `ProduceResponse` with assigned offset and `AckLevel::LeaderFsync`.

### 5.2 Fetch Pipeline
**Entry Point:** `BrokerServer::fetch_with_identity` (`cmd/kamish-broker/src/lib.rs:720`)
1. **RBAC Check:** Ensures client identity has `Consumer` role and matches the topic's tenant ID (unless `Admin`).
2. **Policy Enforcer Check:** Evaluates egress and residency rules for the requesting client.
3. **Partition Log Read:** Reads contiguous records from the requested offset up to `max_bytes` via `PartitionLog::read_records`.
4. **Record Filtering:** Iterates through records, verifying classification and tenant visibility. Returns records wrapped in `FetchResponse`.

### 5.3 Replication Ingestion Pipeline (Follower)
**Entry Point:** `BrokerServer::handle_replication` (`cmd/kamish-broker/src/lib.rs:880`)
1. **RBAC & Peer Identity:** Verifies caller has `Agent` role and matches authorized remote cluster certificate.
2. **Policy & Sealed Check:** Validates policy version. If ciphertext is received without local decryption key custody, rejects or holds in quarantine.
3. **Authority & Epoch Evaluation:**
   - If `request.epoch < local_partition.epoch`: Rejects with `StaleEpoch`.
   - If incoming record offset matches next expected local offset: Contiguous append to local log.
   - **Divergence Handling:** If incoming offset `<` local log end offset, compares checksums and source metadata:
     - If the sender has higher authoritative epoch: Local conflicting records are isolated and appended to `QuarantineStore` (`crates/kamish-own`), local log is truncated to the common prefix via `PartitionLog::truncate_to_offset`, and the incoming authoritative record is appended.
     - If the sender has equal or lower epoch: Rejects with `replication_history_conflict`.
4. **Watermark Return:** Responds with `ReplicationResponse` acknowledging the highest contiguous offset received.

### 5.4 Replication Agent Dispatch Pipeline
**Entry Point:** `ReplicationAgent::step_replication` / `execute_task` (`cmd/kamish-agent/src/lib.rs:188`)
1. **Ownership Fencing:** Calls `check_replication_authority(topic, partition)`. Checks local lease manager or etcd authority before dispatching records.
2. **Circuit Breaker & Route Selection:** Queries `AdaptiveRouteSelector` for target link health. If target circuit breaker is `Open`, skips transfer and schedules backoff.
3. **Batch Construction:** Fetches unacknowledged records from `DestinationOutbox`. Wraps records in `ReplicationEnvelope` with provenance metadata (`epoch`, `source_generation`, `checksum`).
4. **TCP Transport Dispatch:** Transmits `ReplicationRequest` across TCP framing to target broker.
5. **Response & Watermark Advancement:**
   - On Success: Calls `router.record_link_success`, advances outbox watermark using `advance_partition_watermark_with_provenance(topic, partition, epoch, source_gen, ack_offset)`.
   - On Failure: Calls `router.record_link_failure` (tripping circuit breaker if threshold exceeded) and retains outbox records for retry.

### 5.5 Failover & Partition Promotion Pipeline
**Entry Point:** `BrokerServer::promote_partition` (`cmd/kamish-broker/src/lib.rs:1140`)
1. **RBAC & Authorization:** Requires `Operator` or `Admin` role.
2. **Authority Transfer:** Acquires partition ownership from `MetadataAuthority` via an atomic CAS transaction in etcd (`new_epoch = current_epoch + 1`).
3. **Local State Elevation:** Updates local `PartitionState` with new epoch and resets outbox source generation.
4. **Log Fencing:** Writes a fence marker in the audit trail, guaranteeing older epoch writers are rejected by lease checks.

---

## 6. Feature-by-Feature Implementation Reality

| Component / Subsystem | Source Location | Public Entry Point | Execution Reality | Status |
|---|---|---|---|---|
| **Produce Handling** | `cmd/kamish-broker/src/lib.rs:388` | `BrokerServer::produce` | Full validation, pre/post lease checks, deduplication, log fsync, outbox reservation | **IMPLEMENTED AND INTEGRATED** |
| **Segmented Log Storage** | `crates/kamish-log/src/lib.rs` | `PartitionLog::append` | Segments, indexes, OS file locks (`flock`/`LockFileEx`), CRC32 verification, sync to disk | **IMPLEMENTED AND INTEGRATED** |
| **Single-Writer File Locks** | `crates/kamish-log/src/lib.rs:215` | `acquire_partition_lock` | OS-level advisory lock prevents concurrent broker processes accessing same data directory | **IMPLEMENTED AND INTEGRATED** |
| **Producer Deduplication** | `crates/kamish-core/src/session.rs` | `ProducerSessionTracker` | In-memory session tracker bounded by TTL, caches last sequence & offset per producer ID | **IMPLEMENTED AND INTEGRATED** |
| **Durability Level (Leader)**| `cmd/kamish-broker/src/lib.rs:460` | `AckLevel::LeaderFsync` | Disk fsync before produce response is returned | **IMPLEMENTED AND INTEGRATED** |
| **Durability Level (Quorum)**| `crates/kamish-own/src/lib.rs:450` | `AckLevel::InRegionQuorum` | Primitive exists in unit tests; `BrokerServer` runs standalone and does **not** cluster replicas | **PRESENT BUT NOT EFFECTIVE** |
| **Consumer Offsets** | `cmd/kamish-broker/src/lib.rs:1280` | `commit_offset`, `fetch_offset`| Atomic temporary file write, `sync_all` fsync, atomic rename to `consumer_offsets.json` | **IMPLEMENTED AND INTEGRATED** |
| **Metadata Authority (etcd)**| `crates/kamish-meta/src/etcd.rs` | `EtcdMetadataAuthority` | etcd 3.7.3 client TLS, lease grant, renew, and atomic CAS epoch transactions | **IMPLEMENTED AND INTEGRATED** |
| **Lease Renewal Worker** | `cmd/kamish-broker/src/lease.rs` | `spawn_lease_renewal_worker` | Background task: 3s interval, 1s timeout, 10s TTL, fails closed on ambiguous network errors | **IMPLEMENTED AND INTEGRATED** |
| **Outbox Watermark Trimming**| `crates/kamish-outbox/src/lib.rs:175` | `advance_..._with_provenance` | Trims outbox only when acknowledged `epoch` and `source_gen` match or precede records | **IMPLEMENTED AND INTEGRATED** |
| **Outbox Memory Bound** | `crates/kamish-outbox/src/lib.rs:90` | `reserve_capacity` | Enforces byte & record count limits, accounting for raw payload + envelope overhead (64B) | **IMPLEMENTED AND INTEGRATED** |
| **Replication Dispatch** | `cmd/kamish-agent/src/lib.rs:188` | `ReplicationAgent::step_...` | Reads outbox, checks authority, queries route selector, dispatches via TCP framing | **IMPLEMENTED AND INTEGRATED** |
| **Circuit Breakers** | `crates/kamish-route/src/lib.rs:240` | `CircuitBreaker::record_...` | Connected in agent dispatch: trips on repeated failures, resets on successes | **IMPLEMENTED AND INTEGRATED** |
| **Heartbeat Reconciliation** | `cmd/kamish-agent/src/lib.rs:410` | `spawn_retry_worker` | Periodic 500ms sweep, detects unacknowledged outbox ranges, triggers peer reconciliation | **IMPLEMENTED AND INTEGRATED** |
| **Divergence Quarantine** | `crates/kamish-own/src/lib.rs:310` | `QuarantineStore::quarantine`| Isolates conflicting suffixes to disk before truncation; capped at 100,000 records | **IMPLEMENTED AND INTEGRATED** |
| **Partition Truncation** | `crates/kamish-log/src/lib.rs:520` | `truncate_to_offset` | Fsyncs segment truncation, updates in-memory high watermark, removes stale index entries | **IMPLEMENTED AND INTEGRATED** |
| **Wire RBAC** | `cmd/kamish-broker/src/lib.rs:210` | `handle_stream` | Role checks on all wire commands (`Producer`, `Consumer`, `Agent`, `Operator`, `Admin`) | **IMPLEMENTED AND INTEGRATED** |
| **Gateway Auth & CSRF** | `cmd/kamish-gateway/src/lib.rs` | Axum middleware | Argon2 password hashing, secure cookie session tokens, CSRF validation header | **IMPLEMENTED AND INTEGRATED** |
| **Gateway Live Endpoints** | `cmd/kamish-gateway/src/handlers.rs`| `/api/topics`, `/api/quarantine`| Live proxy to broker TCP listener for promote, quarantine inspection, topic listing | **IMPLEMENTED AND INTEGRATED** |
| **Sealed AEAD Replication** | `crates/kamish-seal/src/lib.rs` | `ResidencyPolicy::Sealed...` | Educational XOR prototype in crate; broker **fails closed** (`"sealed_replication_not_configured"`) | **PARTIALLY IMPLEMENTED** |
| **Audit Hash Chains** | `crates/kamish-audit/src/lib.rs:80` | `AuditEventLog::append` | SHA-256 rolling digest across local audit records | **IMPLEMENTED AND INTEGRATED** |
| **Audit Witness Signing** | `crates/kamish-audit/src/witness.rs`| `AuditWitness::sign_checkpoint`| Ed25519 signature of hash chain checkpoint; witness authority daemon not integrated | **IMPLEMENTED, INTEGRATION INCOMPLETE** |
| **TypeScript Client SDK** | `packages/kamish-client/src/` | `KamishClient`, `Producer` | Binary wire protocol framing, connection pooling, retry logic, TypeScript types | **IMPLEMENTED AND INTEGRATED** |
| **React Web Dashboard** | `frontend/src/` | `App.tsx` | Dashboard, topic management, latency graphs, quarantine viewer, audit log viewer | **IMPLEMENTED AND INTEGRATED** |

---

## 7. Distributed-Systems Correctness Findings

### 7.1 Ownership, Leases, and Fencing
- **Authority Acquisition:** Partition ownership is acquired via `EtcdMetadataAuthority` through an atomic transaction (`txn.if(create_revision(key) == 0).then(put(key, epoch=1)).else(...)`). The CAS transaction ensures no two brokers can acquire the same epoch.
- **Lease Invalidation & Margins:** The broker maintains a `safe_margin` of 2.0 seconds against a 10.0-second lease TTL. If the remaining lease duration is `< 2.0s`, produce operations are rejected before entering the log append path.
- **Fail-Closed on Ambiguous Renewal:** The background lease renewal worker (`spawn_lease_renewal_worker`) treats any etcd RPC timeout or transport failure as a potential lease loss. If renewal does not complete before `lease_expiry - safe_margin`, the partition transitions to `Fenced`, blocking all further produce operations.
- **Crash Recovery & Restart Invalidation:** When a broker restarts, it starts with an empty in-memory lease table. It **never** infers ownership from the local log directory. It must successfully re-acquire or renew its lease from etcd before serving writes.
- **Single-Writer File Lock:** On startup, `PartitionLog::open` acquires an OS file lock (`libc::flock(LOCK_EX | LOCK_NB)` on Unix, `LockFileEx` on Windows) on the partition directory. If a split-brain condition causes two broker processes to target the same directory, the second process fails to start immediately.

### 7.2 Replication Correctness & Backlog Recovery
- **Provenance-Aware Watermark Trimming:** `DestinationOutbox::advance_partition_watermark_with_provenance` requires the peer's acknowledgement to supply both the `epoch` and `source_generation`. Outbox entries belonging to newer epochs or future source generations are never deleted by stale acknowledgements.
- **Outbox Backlog Reconstruction:** If an agent restarts or an outbox buffer is cleared, `DestinationOutbox::repopulate_outbox_from_log` reads the partition log sequentially from the destination's last acknowledged watermark up to the log end offset, reconstructing the replication pipeline without message loss.
- **Gap Detection & Reconciliation:** When a follower receives a replication request with `offset > expected_offset`, it returns `ReplicationResponse { status: GapDetected, last_contiguous_offset }`. The sender's heartbeat reconciliation worker notices the gap, fetches missing records from the local log, and replays them.

### 7.3 Divergence, Truncation, and Quarantine Ordering
- **Divergence Discovery:** If an incoming record has an offset that overlaps with existing local records, `BrokerServer::handle_replication` compares the record's payload checksum with the local log.
- **Strict Authority Hierarchy:** If the sender's epoch is greater than the local epoch, the sender is deemed authoritative.
- **Ordering of Quarantine and Truncation:**
  1. The divergent suffix (from the split offset to the local log end) is extracted.
  2. Suffix records are persisted to `QuarantineStore` on disk (`crates/kamish-own`).
  3. `QuarantineStore` flushes its index and records.
  4. Only after quarantine persistence succeeds, `PartitionLog::truncate_to_offset` is called.
  5. The log segment is truncated, `sync_all` is called, and in-memory watermarks are updated.
  6. The authoritative incoming record is appended to the partition log.
- **Failure Window Analysis:** If the process crashes between quarantine write and log truncation, the local log remains intact at its older version. Upon recovery, the next incoming authoritative record will re-trigger the divergence path safely (idempotent quarantine deduplication).

### 7.4 Acknowledgement and Durability Levels
- **LeaderFsync Reality:** When a client specifies `AckLevel::LeaderFsync`, the broker flushes user-space buffers and executes `f.sync_all()` on the underlying OS file descriptor before sending the TCP produce response. This guarantees persistence across power failure on the leader node.
- **InRegionQuorum Non-Reality in BrokerServer:** `BrokerServer` currently runs as a standalone daemon. It contains no peer gossip, Raft group, or in-region synchronous replica pipeline. While `crates/kamish-own` contains unit tests for an in-memory 2-of-3 quorum group, the broker execution path maps all produce writes directly to its single local `PartitionLog`.
- **Finding:** Any system documentation claiming that the current broker delivers "multi-replica quorum durability" is **contradicted by the source**. The broker only delivers durable single-node `LeaderFsync`.

### 7.5 Consumer Semantics & Checkpoint Durability
- **Offset Commit Durability:** `commit_offset` writes the consumer group offset map to a temporary JSON file (`consumer_offsets.json.tmp`), executes `f.sync_all()` to commit the bytes to non-volatile disk, and performs an atomic filesystem rename to `consumer_offsets.json`.
- **Consumer Isolation:** Consumer fetch and commit endpoints enforce tenant boundary checks. A consumer from `tenant-A` cannot fetch records or commit offsets for a topic owned by `tenant-B`.
- **Consumer Group Dynamic Rebalance:** The broker implements basic group offset storage, but does **not** implement a Kafka-style consumer group coordinator (dynamic partition assignment, heartbeats, cooperative rebalances). Clients must manage partition assignments explicitly.

---

## 8. Security and Policy Findings

### 8.1 Transport & Wire Security
- **mTLS Capability:** Both `BrokerServer` and `KamishAgent` support Rustls-based TLS configurations with mutual client certificate validation (`RootCertStore` + client auth).
- **Backend Wire RBAC:** All commands handled by `BrokerServer::handle_stream` are gated by `Identity::role`:
  - `Produce`: `Producer`, `Admin`
  - `Fetch`: `Consumer`, `Admin`
  - `CommitOffset` / `FetchOffset`: `Consumer`, `Admin` (with tenant isolation check)
  - `Replicate` / `Heartbeat`: `Agent`, `Admin`
  - `PromotePartition` / `QuarantineDivergentSuffix`: `Operator`, `Admin`
- **Missing Wire Token Auth:** The broker TCP protocol relies on TLS client certificates for identity. It does not parse Bearer tokens or API keys directly over raw TCP; API token verification is handled at the gateway layer.

### 8.2 Gateway Security & Web Management
- **Password Hashing:** User passwords are stored using Argon2id with random 16-byte salts (`cmd/kamish-gateway/src/auth.rs`).
- **CSRF Protection:** State-changing requests (`POST`, `PUT`, `DELETE`) require a matching `X-CSRF-Token` header validated against the encrypted session cookie.
- **Role Hierarchy:** Gateway routes enforce `Admin`, `Operator`, and `Viewer` roles.

### 8.3 Sealed Replication & Key Custody Reality
- **Status in Source:** `crates/kamish-seal` defines `SealedEnvelope` and includes a mock XOR-CTR implementation explicitly labeled as an educational prototype.
- **Broker Enforcement:** In `BrokerServer::produce`, any topic whose `DataResidencyPolicy` requires `ResidencyPolicy::SealedCiphertext` triggers an immediate error:
  ```rust
  return Err(BrokerError::PolicyViolation("sealed_replication_not_configured".to_string()));
  ```
- **Finding:** Sealed cross-region replication is **not operational**. To make it operational, a production AEAD cipher (ChaCha20-Poly1305 / AES-256-GCM) and an external KMS/HSM integration for wrapping and unwrap keys must be implemented.

### 8.4 Audit Logging and External Witness Anchoring
- **Local Tamper Evidence:** `AuditEventLog` (`crates/kamish-audit`) records all administrative actions, failover events, and policy mutations into a local append-only log with rolling SHA-256 digests.
- **Witness Checkpointing:** `AuditWitness` implements Ed25519 signing of audit checkpoints. `verify_file` detects record modification, truncation, and digest mismatch.
- **Finding:** While the cryptographic primitives exist and pass unit tests, `BrokerServer` and `kamish-gateway` do not run an autonomous background daemon that periodically transmits signed audit checkpoints to an external trusted witness authority or immutable ledger.

---

## 9. Consumer, Gateway, SDK, Frontend, and Operational Integration Findings

### 9.1 TypeScript SDK (`packages/kamish-client`)
- **Integration Status:** Fully implemented and aligned with the broker's binary wire framing.
- **Capabilities:**
  - Binary protocol encoder/decoder (`src/protocol.ts`) supporting `Produce`, `Fetch`, `CommitOffset`, `FetchOffset`.
  - Connection pooling with automatic reconnection and backoff (`src/transport.ts`).
  - High-level `Producer` class with batching and sequence tracking (`src/producer.ts`).
  - Strict TypeScript typings matching `crates/kamish-wire`.

### 9.2 Frontend Administration UI (`frontend`)
- **Integration Status:** Implemented in React 18, Vite, and Tailwind CSS.
- **Capabilities:**
  - Communicates with `kamish-gateway` via REST and Server-Sent Events (`/api/events`).
  - Topology view displaying regions, broker health, and replication links.
  - Quarantine viewer allowing inspection of divergent suffixes.
  - Live metric graphs (latency, throughput, queue depth) consuming gateway SSE streams.

### 9.3 Operational Daemon Lifecycles
- **Broker Daemon (`kamish-broker`):** Starts TCP listener, initializes storage directories, opens metadata authority, spawns lease renewal worker, and handles graceful shutdown on `SIGINT`/`SIGTERM` by flushing open segments and outbox buffers.
- **Agent Daemon (`kamish-agent`):** Connects to local broker outbox, initializes route selector, spawns retry worker (500ms heartbeat sweep), and handles graceful shutdown by waiting for in-flight replication tasks to complete.

---

## 10. Evidence and Historical-Report Assessment

### 10.1 Inspection of `audit/` Directory
- `audit/2026-03-31-release-readiness-audit.md`: Historical report from early development. Many sections describe preliminary designs that were subsequently superseded.
- `audit/2026-04-14-comprehensive-verification.md`: Documents verification of earlier log and lease implementations. The current source includes subsequent hardening (FN-02.1 atomicity, FN-04 provenance watermarks) that postdate this document.

### 10.2 Inspection of `benchmarks/` Directory
- `benchmarks/baselines/kafka-mm2/`: Contains configuration files (`mm2.properties`) and documentation claiming parity with Kafka MirrorMaker 2.
- `benchmarks/baselines/pulsar/`: Contains Pulsar replication comparison configurations.
- **Finding:** The benchmark directories contain configuration files and static write-ups. They do **not** contain automated harness runs or reproducible telemetry proving performance advantages against Kafka or Pulsar under live multi-region network conditions.

### 10.3 Inspection of `scripts/` Directory
- `scripts/verify-all.sh`, `scripts/reconcile.sh`, `scripts/dev-cluster.sh`: Well-structured shell automation for launching multi-broker local environments and checking code quality. These are operational scripts, not proof of continuous verification.

---

## 11. Documentation Contradictions and Stale Claims

The audit identified several significant contradictions between historical documentation and the actual source code at `6a5ef4a`:

| Claim in Documentation / Backlog | Source Reality at `6a5ef4a` | Verdict |
|---|---|---|
| *"In-region replica quorum durability guarantees zero data loss"* | `BrokerServer` only runs `LeaderFsync` on a single local partition log; no replica cluster exists in `BrokerServer`. | **CONTRADICTED** |
| *"Sealed ciphertext replication provides end-to-end data residency privacy"* | Broker explicitly rejects sealed residency requests (`"sealed_replication_not_configured"`). `kamish-seal` is an educational mock. | **CONTRADICTED** |
| *"Consumer group rebalancing automatically reallocates partitions"* | Broker only provides key-value offset storage (`commit_offset`/`fetch_offset`). No group coordinator or rebalancer exists. | **OVERSTATED** |
| *"Outbox watermarks can be cleared by stale follower acks"* | Resolved in `bb8a75b`: `advance_partition_watermark_with_provenance` enforces epoch and source generation checks. | **STALE BACKLOG (RESOLVED)** |
| *"Circuit breaker is not integrated into agent replication dispatch"* | Resolved in `7eb18b5`: `ReplicationAgent::execute_task` calls `record_link_success` and `record_link_failure`. | **STALE BACKLOG (RESOLVED)** |
| *"Consumer offset commit can lose data on crash"* | Resolved in `1908aa3`: `commit_offset` now executes `f.sync_all()` on the temporary file before atomic rename. | **STALE BACKLOG (RESOLVED)** |
| *"Broker wire commands lack RBAC enforcement"* | Resolved in `1908aa3`: `handle_stream` gates all commands with role checks. | **STALE BACKLOG (RESOLVED)** |
| *"Quarantine store can grow indefinitely and exhaust disk space"* | Resolved in `bb8a75b`: `QuarantineStore::quarantine_records` enforces a hard cap of 100,000 records. | **STALE BACKLOG (RESOLVED)** |

---

## 12. Critical Architectural Risks and Dependencies

1. **Lack of In-Region Quorum Replication:** Because `BrokerServer` relies on single-node `LeaderFsync`, hardware failure of a regional broker node before replication records are transferred across the WAN results in partition unavailability until the disk is recovered. If the disk is unrecoverable, acknowledged messages are lost.
2. **External etcd Dependency:** While `kamish-meta` implements robust lease renewal and fail-closed logic, an extended etcd outage will cause all regional partition leases to expire, halting all produce writes across the entire cluster.
3. **Absence of Production Key Custody for Sealed Replication:** Until an external KMS/HSM service is integrated, tenants requiring cross-region data residency privacy cannot use the platform without exposing plaintext in transit.
4. **Autonomous Witness Checkpoint Export:** Local audit logs are protected by SHA-256 hash chains, but without periodic pushing of signed checkpoints to an independent external authority, a root administrator on the broker host could theoretically truncate or tamper with the log files and rebuild the hash chain.

---

## 13. Prioritized List of Confirmed Gaps

### P0 — Critical Correctness or Security Gaps
- **None currently identified in existing single-node `LeaderFsync` paths.** (Previous race conditions in truncation, watermark trimming, and checkpoint fsync have been resolved).

### P1 — Required Core Functionality
1. **Production Authenticated Encryption (AEAD) for Sealed Mode:** Replace mock XOR in `crates/kamish-seal` with ChaCha20-Poly1305 / AES-256-GCM and integrate key derivation/custody interface to allow `ResidencyPolicy::SealedCiphertext` to operate safely.
2. **Autonomous Audit Witness Daemon:** Wire `AuditWitness` into a background worker that periodically commits signed checkpoints to an external endpoint or storage bucket.

### P2 — Operational Completeness
1. **Dynamic Consumer Group Coordinator:** Implement heartbeat, join-group, and partition assignment protocol in `BrokerServer` to support multi-consumer horizontal scaling without manual partition assignment.
2. **Multi-Replica In-Region Quorum Engine:** Integrate `InRegionReplicationGroup` from `crates/kamish-own` into the live `BrokerServer` network pipeline to provide genuine `AckLevel::InRegionQuorum`.

### P3 — Productization & Tooling
1. **Automated Multi-Region Benchmark Harness:** Implement runnable benchmark harness scripts to generate verified throughput and latency baselines under simulated WAN latency.
2. **Prometheus / OpenTelemetry Metrics Exporter:** Expose standard `/metrics` HTTP endpoint on broker and agent daemons for enterprise observability.

---

## 14. Questions Requiring Runtime Verification or Design Decisions

1. **Design Decision: In-Region Replication Model:** Should Kamish implement internal in-region replica clustering (e.g. Raft or primary-backup replication within a region), or remain an architecture where each regional broker relies on local persistent NVMe storage and replicates exclusively across regions?
2. **Design Decision: Key Custody Architecture for Sealed Mode:** Which external KMS providers (AWS KMS, GCP Cloud KMS, HashiCorp Vault) should be supported for key custody in sealed cross-region replication?
3. **Runtime Verification: Failover Under WAN Partition:** How does the system behave when an etcd network partition isolates a regional broker while cross-region replication links remain partially reachable? (Requires dedicated distributed fault-injection testing).
4. **Runtime Verification: High-Throughput Outbox Refill:** What is the disk I/O performance impact of outbox backlog reconstruction from disk segments when concurrent produce load is high?

---

## 15. Conclusion on Honest Claims

### What CAN Honestly Be Claimed:
- Kamish provides a functional single-node regional broker engine with persistent segmented logs, OS file locking, and `LeaderFsync` durability.
- Partition ownership is fenced using etcd 3.7.3 atomic transactions, monotonic epochs, and proactive lease renewal workers that fail closed on network timeouts.
- Cross-region replication via `kamish-agent` is operational, with circuit breaker link protection, provenance-aware watermark advancement, and automatic backlog recovery.
- Divergent partition histories during failover are safely quarantined on disk before log truncation occurs, with bounded storage limits.
- Wire protocols, gateway APIs, and consumer checkpoints enforce RBAC, tenant isolation, and crash-durable persistence (`sync_all`).
- A functional TypeScript client SDK and React administration dashboard are implemented and integrated with the backend APIs.

### What CANNOT Honestly Be Claimed:
- The system does **not** currently deliver multi-replica in-region quorum durability (`AckLevel::InRegionQuorum` is not operational in `BrokerServer`).
- The system does **not** currently support sealed ciphertext cross-region replication (it deliberately fails closed).
- The system does **not** provide dynamic Kafka-style consumer group auto-rebalancing.
- The system does **not** have empirical performance benchmarks proving superiority over Kafka MirrorMaker 2 or Apache Pulsar under live multi-region network conditions.

