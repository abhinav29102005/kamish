# KAMISH — IMPLEMENTATION INVENTORY

**Inspected Revision:** `6a5ef4a4802e613441cdc8a3ce5e225add236a2c` (Branch: `main`)  
**Date:** 2026-11-01  
**Method:** Static Source Inspection, Git Commit History, Manifest & Configuration Audit

---

## 1. Classification Methodology

Each entry in this inventory represents a distinct capability within the Kamish codebase, evaluated against the strict classification schema mandated by the Deep Reconnaissance directive:

- **IMPLEMENTED AND INTEGRATED:** The relevant source and complete execution path support the claimed functionality end-to-end.
- **IMPLEMENTED, INTEGRATION INCOMPLETE:** Core logic exists, but one or more required callers, lifecycle paths, or dependent components are missing or unhooked.
- **PARTIALLY IMPLEMENTED:** Some of the claimed behavior exists, but important requirements or edge cases remain absent.
- **PRESENT BUT NOT EFFECTIVE:** Code exists but does not establish the claimed outcome because of a bypass, incorrect ordering, missing persistence, or lack of runtime clustering.
- **DOCUMENTED ONLY:** The behavior is specified in architectural documents or specifications, but not implemented in source code.
- **NOT FOUND:** No adequate implementation was found in the repository.
- **SUPERSEDED:** Later source changes replaced the earlier implementation, API, or contract.
- **CONTRADICTED:** Current source or runtime reality conflicts directly with the documented claim.
- **UNVERIFIABLE FROM AVAILABLE EVIDENCE:** The static source does not permit a defensible conclusion without dedicated distributed runtime verification.

---

## 2. Comprehensive Subsystem Inventory Table

| ID | Capability | Source locations | Integration status | Evidence class | Current status | Remaining gap | Dependency/blocker |
|:---|:---|:---|:---|:---|:---|:---|:---|
| **LOG-01** | Segmented log file storage with active and sealed segments | `crates/kamish-log/src/lib.rs:80` | Fully connected in broker append/read path | Source inspection | **IMPLEMENTED AND INTEGRATED** | None for single-process broker | Disk space |
| **LOG-02** | Log index mapping record offsets to physical byte positions | `crates/kamish-log/src/lib.rs:140` | Fully connected during write and random read | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | Local filesystem |
| **LOG-03** | Single-writer OS file lock on partition directories | `crates/kamish-log/src/lib.rs:215` | Invoked on `PartitionLog::open` (`flock`/`LockFileEx`) | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | OS file lock support |
| **LOG-04** | Cyclic Redundancy Check (CRC32) per record payload | `crates/kamish-log/src/lib.rs:280` | Verified on append and read scan | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | CPU cycles |
| **LOG-05** | Atomic log truncation to common prefix offset | `crates/kamish-log/src/lib.rs:520` | Called during failover reconciliation with `sync_all` | Source inspection, Git `41e6b49` | **IMPLEMENTED AND INTEGRATED** | None | Filesystem rename/truncate |
| **LOG-06** | Segment log retention and background garbage collection | `crates/kamish-log/src/lib.rs:610` | Manual trigger implemented; periodic background cleaner worker not spawned | Source inspection | **IMPLEMENTED, INTEGRATION INCOMPLETE** | Needs background timer sweep daemon in broker | Timer infrastructure |
| **DUR-01** | Local persistent log durability (`LeaderFsync`) | `cmd/kamish-broker/src/lib.rs:460` | Fsyncs segment before returning produce ACK | Source inspection, Git `0895dd6` | **IMPLEMENTED AND INTEGRATED** | None for single leader | NVMe write barrier |
| **DUR-02** | In-region multi-replica synchronous quorum durability | `crates/kamish-own/src/lib.rs:450`, `crates/kamish-core/src/lib.rs` | Enum exists; group tested in unit tests, but `BrokerServer` runs single-node | Source inspection, Unit tests | **PRESENT BUT NOT EFFECTIVE** | BrokerServer lacks replica network clustering | In-region cluster engine |
| **DUR-03** | Cross-region synchronous produce durability | `docs/CONTRACT.md`, `crates/kamish-core/src/lib.rs` | Not wired in produce path (produce returns after local leader write) | Source inspection | **DOCUMENTED ONLY** | Synchronous WAN wait path not implemented | Cross-region latency SLA |
| **DED-01** | Producer session tracking and idempotent deduplication | `crates/kamish-core/src/session.rs:45`, `cmd/kamish-broker/src/lib.rs:420` | Checked and updated before every log append | Source inspection | **IMPLEMENTED AND INTEGRATED** | In-memory only; sessions reset across broker restart | Disk session log |
| **OWN-01** | etcd 3.7.3 atomic CAS ownership acquisition & epoch bump | `crates/kamish-meta/src/etcd.rs:110` | Transaction checks create revision before granting epoch | Source inspection, Git `75165c5` | **IMPLEMENTED AND INTEGRATED** | None | Running etcd 3.7.3 cluster |
| **OWN-02** | Local lease validity check with 2.0s safe margin | `cmd/kamish-broker/src/lib.rs:340` | Checked pre-append and post-append in `produce` | Source inspection, Git `0895dd6` | **IMPLEMENTED AND INTEGRATED** | None | Monotonic clock |
| **OWN-03** | Proactive background lease renewal worker daemon | `cmd/kamish-broker/src/lease.rs:60` | 3s interval, 1s timeout, 10s TTL, fails closed on error | Source inspection, Git `baab371` | **IMPLEMENTED AND INTEGRATED** | None | Tokio runtime |
| **OWN-04** | Previous-owner epoch fencing on write and dispatch | `cmd/kamish-broker/src/lib.rs:360`, `cmd/kamish-agent/src/lib.rs:160` | Checked before accepting produce and before agent dispatch | Source inspection, Git `9f6dd63` | **IMPLEMENTED AND INTEGRATED** | None | Monotonic epoch |
| **OWN-05** | Partition failover promotion via administrative endpoint | `cmd/kamish-broker/src/lib.rs:1140`, `cmd/kamish-gateway/src/handlers.rs` | Gateway forwards promote command to broker TCP listener | Source inspection, Git `01e183f` | **IMPLEMENTED AND INTEGRATED** | None | Operator credential |
| **OUT-01** | Per-destination outbox staging with byte/record bounds | `crates/kamish-outbox/src/lib.rs:85` | Capacity checked and reserved during `produce` | Source inspection, Git `baab371` | **IMPLEMENTED AND INTEGRATED** | None | RAM buffer limits |
| **OUT-02** | Provenance-aware outbox watermark advancement | `crates/kamish-outbox/src/lib.rs:175` | Watermark advances only when epoch & source_gen match/lead | Source inspection, Git `bb8a75b` | **IMPLEMENTED AND INTEGRATED** | None | Follower ack payload |
| **OUT-03** | Outbox backlog reconstruction from persistent log | `crates/kamish-outbox/src/lib.rs:240` | Scans log from last acked watermark on startup/refill | Source inspection, Git `baab371` | **IMPLEMENTED AND INTEGRATED** | None | Segment availability |
| **REP-01** | Cross-region replication envelope with full context | `crates/kamish-core/src/lib.rs:320`, `cmd/kamish-agent/src/lib.rs` | Carries epoch, source_gen, checksum, timestamp, policy | Source inspection, Git `cbc426b` | **IMPLEMENTED AND INTEGRATED** | None | Wire protocol |
| **REP-02** | Follower contiguous append and gap detection | `cmd/kamish-broker/src/lib.rs:890` | Verifies offset sequence, appends to log, returns ACK/gap | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | TCP link |
| **REP-03** | Divergent history detection and quarantine routing | `cmd/kamish-broker/src/lib.rs:940`, `crates/kamish-own/src/lib.rs:310` | Conflicting suffix written to QuarantineStore before truncate | Source inspection, Git `41e6b49`, `0895dd6` | **IMPLEMENTED AND INTEGRATED** | None | Disk space |
| **REP-04** | Quarantine store bounded growth (100k records) | `crates/kamish-own/src/lib.rs:335` | Safely drops oldest entries if runaway divergence occurs | Source inspection, Git `bb8a75b` | **IMPLEMENTED AND INTEGRATED** | None | RAM/disk bounds |
| **REP-05** | Heartbeat reconciliation daemon (500ms sweep) | `cmd/kamish-agent/src/lib.rs:410` | Detects gaps and un-replicated ranges, replays backlog | Source inspection, Git `baab371`, `9f6dd63` | **IMPLEMENTED AND INTEGRATED** | None | Network connectivity |
| **ROU-01** | Adaptive route selector with link latency tracking | `crates/kamish-route/src/lib.rs:110` | Records RTT and routes traffic via lowest-cost path | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | Ping/ACK telemetry |
| **ROU-02** | Circuit breaker integration on agent dispatch | `crates/kamish-route/src/lib.rs:240`, `cmd/kamish-agent/src/lib.rs:210` | `record_link_success` / `record_link_failure` wired into agent | Source inspection, Git `7eb18b5` | **IMPLEMENTED AND INTEGRATED** | None | Threshold tuning |
| **CNS-01** | Consumer offset commit and fetch API | `cmd/kamish-broker/src/lib.rs:1280` | Client commits group offsets; broker stores in JSON file | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | Client coordination |
| **CNS-02** | Crash-durable consumer offset fsync | `cmd/kamish-broker/src/lib.rs:1305` | Calls `f.sync_all()` on tmp file before atomic rename | Source inspection, Git `1908aa3` | **IMPLEMENTED AND INTEGRATED** | None | Local filesystem |
| **CNS-03** | Tenant isolation on consumer offset operations | `cmd/kamish-broker/src/lib.rs:1290` | Prevents unauthorized tenant reading/writing another's offsets | Source inspection, Git `1908aa3` | **IMPLEMENTED AND INTEGRATED** | None | Identity token |
| **CNS-04** | Dynamic consumer group partition coordinator & rebalancer | `crates/kamish-core/src/lib.rs` | Not implemented in broker; no heartbeat or rebalance protocol | Source inspection | **NOT FOUND** | Needs complete join/sync group state machine | Distributed protocol |
| **SEC-01** | Transport Layer Security (mTLS) on broker and agent | `cmd/kamish-broker/src/main.rs`, `cmd/kamish-agent/src/main.rs` | Rustls client cert verification and root store loading | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | PKI / X.509 certs |
| **SEC-02** | Comprehensive wire RBAC on broker TCP commands | `cmd/kamish-broker/src/lib.rs:210` | Enforces roles on Produce, Fetch, Commit, Replicate, Promote | Source inspection, Git `1908aa3` | **IMPLEMENTED AND INTEGRATED** | None | Identity context |
| **SEC-03** | Gateway session authentication with Argon2id | `cmd/kamish-gateway/src/auth.rs:75` | Secure password hashing, salt generation, session tokens | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | Gateway DB |
| **SEC-04** | Gateway CSRF protection middleware | `cmd/kamish-gateway/src/middleware.rs:40` | Validates `X-CSRF-Token` against session cookie | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | HTTP headers |
| **SEC-05** | Sealed ciphertext replication (AEAD) | `crates/kamish-seal/src/lib.rs` | XOR mock in crate; broker fails closed with policy error | Source inspection, Git `1908aa3` | **PARTIALLY IMPLEMENTED** | Needs real AEAD (ChaCha20) and KMS key custody | External KMS/HSM |
| **SEC-06** | Audit trail with rolling SHA-256 hash chains | `crates/kamish-audit/src/lib.rs:80` | Local append-only log with cryptographic integrity digests | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | Local storage |
| **SEC-07** | Audit witness Ed25519 signature verification | `crates/kamish-audit/src/witness.rs:60` | Checks for modification, truncation, and digest mismatch | Source inspection | **IMPLEMENTED, INTEGRATION INCOMPLETE** | Autonomous background witness publisher daemon missing | External witness node |
| **SDK-01** | TypeScript Client SDK connection pool & wire framing | `packages/kamish-client/src/transport.ts` | Handles binary protocol over TCP socket with reconnects | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | Node.js runtime |
| **SDK-02** | TypeScript Producer with sequence tracking & batching | `packages/kamish-client/src/producer.ts` | High-level API for sending records with deduplication | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | TypeScript SDK |
| **UI-01** | React administration dashboard with live SSE streaming | `frontend/src/App.tsx`, `cmd/kamish-gateway/src/handlers.rs` | Displays cluster topology, topic list, and live SSE metrics | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | Web browser |
| **UI-02** | Frontend quarantine suffix inspection and manual replay | `frontend/src/components/QuarantineView.tsx` | UI queries `/api/quarantine`, allows operator inspection | Source inspection, Git `01e183f` | **IMPLEMENTED AND INTEGRATED** | None | Gateway proxy |
| **OPS-01** | Graceful daemon shutdown with buffer flushing | `cmd/kamish-broker/src/main.rs`, `cmd/kamish-agent/src/main.rs` | Catches SIGINT/SIGTERM, flushes log segments and outbox | Source inspection | **IMPLEMENTED AND INTEGRATED** | None | OS signal handling |
| **OPS-02** | Automated multi-region performance benchmarks | `benchmarks/baselines/` | Properties files present; automated runner and live metrics absent | Static directory audit | **DOCUMENTED ONLY** | Automated test harness scripts not implemented | Benchmark cluster |

