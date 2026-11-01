# Independent Audit Report: Phase 5 Verification (Dashboard with Real Connectivity)

**Audit Date:** 2026-11-01  
**Auditor:** Antigravity Autonomous Security & Correctness Audit Subsystem  
**Target Repository:** `abhinav29102005/kamish` (`/home/bigboyaks/Projects/crowded-mausoleum`)  
**Scope:** Strict read-only audit of Phase 5 (Gateway and Dashboard connectivity), phase dependencies (Phases 1–4), and codebase integrity.  
**Audit Artifacts Directory:** `audit/phase5_verification_20261101/`

---

## Executive Summary & Overall Verdict

| Metric | Status |
| --- | --- |
| **Phase 5 Overall Verdict** | **PARTIAL** |
| **Code Modification During Audit** | **NONE (Strict Read-Only Enforcement)** |
| **Workspace Unit & Integration Tests** | **221 PASSED / 0 FAILED** |
| **Research Probes** | **FAILED (Compilation Broken due to API Desynchronization)** |
| **Cargo Lints (Clippy -D warnings)** | **FAILED (1 error in `crates/kamish-outbox`)** |
| **Code Formatting (Cargo Fmt)** | **FAILED (Diffs in `cmd/kamish-agent`)** |
| **Frontend Test & Build Suite** | **PASSED (4 tests passed, TypeScript clean, Vite build clean)** |
| **Node.js SDK Suite** | **PASSED (23 unit tests + 1 integration test passed)** |
| **Playwright Browser E2E Suite** | **8/8 PASSED** |

### Section Verdicts (A through E)
1. **Section A (Gateway Architecture & Security): PARTIAL** — Axum gateway exists with real session auth, CSRF tokens, IP rate limiting, and RBAC, but embeds the broker in-process (no network client), omits the `Secure` cookie attribute, lacks cross-tenant isolation enforcement, has hardcoded passwords, and omits CSP/HSTS headers.
2. **Section B (Real Data Connectivity): PARTIAL** — Partition produce/fetch, high watermark, broker region, and degraded link tracking are real; however, outbox depth is absent from the API and 100% simulated in client state, canary/checker results are hardcoded mock JSON, and verify-audit returns a static pass without re-validating ledger signatures on disk.
3. **Section C (Operations & Failures): PARTIAL** — Topic creation, produce, fetch, and mode toggling perform real state modifications; however, failure injection scenarios (`residency`, `outage`, `quorum`) return hardcoded static strings without executing real chaos tests, and broker process failure cannot be decoupled from gateway failure.
4. **Section D (Frontend Integrity): PARTIAL** — Complete dedicated brutalist UI with real REST endpoints; however, client-side timers continuously generate synthetic latency (`Math.random()`), queue depth, and traffic EPS that overlay live mode, 3D dragon assets are 53MB uncompressed without reduced motion support, and initial ledger history loads mock entries.
5. **Section E (Honesty & Documentation Integrity): PASS (with caveats)** — Documentation rigorously disclaims production readiness, legal compliance, global exactly-once delivery, and sealed replication; however, gateway scenario strings claim "3-replica quorum verified" despite the absence of an in-region replica quorum.

---

## 1. Environment & Setup Reconciliation

### 1.1 Git Snapshot & Branch Alignment
- **Local HEAD Commit:** `569f31baedad522e39e9679d9e81ba324a61767c` (`569f31b`)
- **Active Branch:** `main`
- **Working Tree Status:** Clean (`git status --short` returned empty).
- **Origin Tracking Status:** **DIVERGED**.
  - Local `main` contains 1 unpushed commit (`569f31b`: *"redesign: center all ui elements, refine navbar with settings popover and remove sign-in button"*).
  - Remote `origin/main` contains 5 commits not present in local HEAD:
    - `5c4b011`: *docs: add consolidated functional completion reconnaissance report and section 33*
    - `a9586b3`: *Merge branch 'main' of https://github.com/abhinav29102005/kamish*
    - `01e183f`: *feat(gateway): add partition promotion, quarantine inspection, and reconciliation endpoints*
    - `9f6dd63`: *feat(agent): enforce ownership fencing on replication dispatch and wire retry worker*
    - `0895dd6`: *feat(broker): implement explicit ack durability levels and authoritative divergence quarantine*
- **Commits Since Last Audited Commit (`6e4666a`):** Exactly 34 commits in local history.

### 1.2 Test Suite Execution Summary
All raw, unedited test outputs have been preserved in `audit/phase5_verification_20261101/`:

| Test Suite | Command | Exit Code | Result | Evidence File |
| --- | --- | --- | --- | --- |
| **Cargo Fmt** | `cargo fmt --all -- --check` | 1 | **FAIL** (Formatting diffs in `cmd/kamish-agent/src/lib.rs`) | `cargo_fmt.txt` |
| **Cargo Clippy** | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 101 | **FAIL** (`clippy::too-many-arguments` in `crates/kamish-outbox/src/lib.rs:87:5`) | `cargo_clippy.txt` |
| **Workspace Unit/Int** | `cargo test --workspace --all-features` | 0 | **PASS** (221 passed, 0 failed, 0 ignored across 13 packages) | `cargo_test.txt` |
| **Research Probes** | `cargo test` in `early_research/probes/` | 101 | **FAIL** (Compile error: missing `checksum`, `context`, `envelope_payload` in `ReplicationRequest`) | `probes.txt` |
| **Gateway Tests** | `cargo test -p kamish-gateway` | 0 | **PASS** (9 passed in `tests/gateway_api.rs`) | `gateway_tests.txt` |
| **Frontend Pipeline** | `tsc -b && oxlint && node --test && vite build` | 0 | **PASS** (4 tests passed, 0 errors, 17 linter warnings, build time ~320ms) | `frontend_checks.txt` |
| **Client SDK** | `tsc -b && node --test && npm pack --dry-run` | 0 | **PASS** (23 unit tests passed, 1 integration test passed, package tarball verified) | `sdk_tests.txt` |
| **Playwright E2E** | `python3 scripts/test_dashboard_e2e.py` | 0 | **PASS** (8/8 test suites passed against live gateway and Vite preview) | `playwright_e2e.txt` |
| **Live E2E Demo** | Integrated live broker/gateway boot, produce, fetch, restart, verify, logout | 0 | **PASS** (Demonstrated on port 3099) | `end_to_end_demo.txt` |

---

## 2. Phase Dependency Check (Phases 1 to 4)

| Component | Status | Verification & Evidence |
| --- | --- | --- |
| **Phase 1: Etcd MetadataAuthority** | **PARTIAL** | `crates/kamish-meta/src/etcd.rs` implements the consensus authority with pure-Rust TLS ring mTLS. However, in standard `cargo test` runs, all 12 tests in `crates/kamish-meta/tests/etcd_authority.rs` are **skipped** unless `KAMISH_ETCD_TESTS=1` is manually exported. Furthermore, the early research probes in `early_research/probes` **fail to compile**, and probe `audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable` in `remaining_gaps.rs` was tampered with (asserting `AuditLog::open` failure instead of demonstrating gap detection). |
| **Phase 2: TLS 1.3 mTLS & Roles** | **PARTIAL** | Broker-to-broker and client-to-broker TCP framing supports real TLS 1.3 mTLS with client certificate SAN extraction (`cmd/kamish-broker/src/tls.rs`, tested in `cmd/kamish-broker/tests/tls_mutual_auth.rs`). However, the HTTP Dashboard Gateway (`cmd/kamish-gateway`) runs over plaintext HTTP (no TLS listener), lacks the `Secure` cookie attribute, and has hardcoded credentials in memory. |
| **Phase 3: Durability & Replica Quorum** | **PARTIAL** | Local log append and partition indexing on disk are real (`crates/kamish-log`). WAN replication via the outbox is asynchronous. However, **no in-region replica quorum exists**; each region runs a single broker daemon against local disk segments. Gateway scenario strings claiming "2-of-3 in-region replica quorum verified" are simulated. |
| **Phase 4: Invariant Checkers & Canary Scanner** | **PARTIAL** | `crates/kamish-checkers` contains real, rigorous invariant checkers (INV-1 through INV-9) and a canary leak scanner. However, the Gateway endpoint `/api/checkers` **does not execute this library**; it returns a static, hardcoded mock JSON payload with 0 violations and a dummy token string. |

---

## 3. Phase 5 Detailed Verification Checklist (Sections A through E)

### Section A: Gateway Architecture & Security
1. **Language, Location & Broker Communication:**
   - Written in Rust using Axum 0.8 and Tower 0.5, located in `cmd/kamish-gateway/`.
   - **Critical Architectural Finding:** The gateway does **not** connect as an external client over TCP/TLS to an independent broker daemon. Instead, `cmd/kamish-gateway/src/main.rs` directly constructs an embedded `Arc<BrokerServer>` in-process. Axum handlers call `state.broker` directly.
2. **Authentication & Session Mechanics:**
   - Password Hashing: Implements a custom PBKDF2-like loop in `cmd/kamish-gateway/src/auth.rs` (10,000 rounds of SHA-256 with a 16-byte random salt). Note: This is an internal custom loop rather than standard Argon2id or `ring::pbkdf2`.
   - Session Cookie: `kamish_session=<hex>; HttpOnly; SameSite=Strict; Path=/; Max-Age=3600`.
   - **Defect:** The **`Secure`** cookie attribute is **omitted**.
   - CSRF Defense: Enforced on all `POST`, `PUT`, and `DELETE` methods via the `x-csrf-token` header matching `session.csrf_token`. Tested and verified in `test_csrf_protection_and_bypass_prevention`.
   - Rate Limiting: Sliding-window in-memory rate limiter per client IP (120 req/minute). Verified in `test_sliding_window_rate_limiting`.
   - Session Expiry: 3,600s TTL validated in `validate_session`. Logout explicitly invalidates the session in memory and sets `Max-Age=0`.
3. **Authorization & RBAC:**
   - Roles: `Admin`, `Operator`, `Auditor`, `Producer`, `Consumer`.
   - Denied Operation: Tested in `test_role_based_access_control` (Consumer denied produce -> HTTP 403; Consumer denied policy publish -> HTTP 403).
   - **Defect:** **Cross-tenant requests are NOT rejected**. `produce_handler` and `fetch_handler` pass `None` as the client identity to `broker.dispatch_with_identity(...)`. The `Session` struct contains no `tenant_id`, and no tenant validation test exists in `gateway_api.rs`.
4. **Input Validation, Headers & CORS:**
   - CORS explicitly allows credentials and restricts origins to local dev hosts.
   - Headers: `x-content-type-options: nosniff` and `x-frame-options: DENY` applied.
   - **Defects:** `Content-Security-Policy` (CSP) and `Strict-Transport-Security` (HSTS) headers are missing. Axum uses default body size limits (2MB).
5. **Secrets & Credentials:**
   - Hardcoded in source code: `UserStore::new_with_defaults()` registers preconfigured plaintext passwords in `cmd/kamish-gateway/src/auth.rs`. No environment variable, file secret, or external vault loader exists.
   - Ephemeral signing key: `create_default_policy_cache` generates a new, ephemeral Ed25519 signing key on every process startup.

---

### Section B: Data Reality Classification

| Telemetry / Field | Reality Status | Actual Source & Mechanism |
| --- | --- | --- |
| **Broker Status** | **REAL / DERIVED** | `/api/status` returns real region, known peers, degraded link status, and resource limits from `BrokerServer`. (Note: status string is hardcoded to `"HEALTHY"`). |
| **Per-stream Watermark** | **REAL** | High watermarks returned by `/api/streams` and `/api/fetch` come directly from on-disk partition logs. |
| **Replication Lag** | **DERIVED** | Calculated in `/api/streams` from local partition high watermark minus peer watermark in `LocalReplicationTracker`. |
| **Outbox Depth** | **SIMULATED / ABSENT** | **Absent from Gateway API**. In the UI, `KamishContext.tsx` generates synthetic queue depth via an in-memory counter (`eu.outboxQueueSize += increment`). Displays as "SIMULATED QUEUE DEPTH". |
| **Ownership / Epoch / Lease** | **REAL** | `/api/authority` returns active partition ownership epoch and local renewal lease status from `state.broker.meta`. |
| **Policy Version & Freshness** | **REAL** | `/api/policy` queries `state.broker.meta.get_policy_floor(&cluster_id)`. Updates via `/api/policy/publish` advance the floor. |
| **Audit Decisions** | **REAL / DERIVED** | `/api/audit` reads real checkpoint records (`stream`, `tail_hash`, `record_count`) committed by `kamish_audit` to `state.broker.meta`. |
| **Verify-Audit Result** | **SIMULATED** | `/api/audit/verify` only counts if checkpoints exist in metadata. It **does not** verify the on-disk cryptographic hash chain or signatures, returning static `"cryptographic_signatures_valid": true, "zero_tamper_detected": true`. |
| **Canary / Checker Results** | **SIMULATED** | `/api/checkers` returns static, hardcoded JSON (0 violations, dummy canary token). It does not invoke `crates/kamish-checkers`. |
| **Route State** | **DERIVED** | `/api/routing` returns circuit breaker state and synthetic latency (350.0ms if degraded, 70.0ms if healthy) derived from `broker.is_wan_degraded(peer)`. |

---

### Section C: Operational Integrity & Failures
1. **Real State Modifications:**
   - `POST /api/topics`: Registers topic metadata into `TenantCatalog` and initializes broker ownership.
   - `POST /api/produce`: Durably commits record to local partition log on disk and returns base offset and epoch.
   - `POST /api/fetch`: Reads persisted records from disk at requested offset.
   - `POST /api/policy/publish`: Advances the durable policy version floor in metadata.
   - `POST /api/mode`: Toggles global runtime flag and broadcasts SSE notification.
2. **Failure Paths & Error Transparency:**
   - Unauthenticated requests receive HTTP 401 with JSON error.
   - CSRF omissions or mismatches receive HTTP 403.
   - Unauthorized role operations receive HTTP 403.
   - Rate limit exhaustion receives HTTP 429.
   - **Defect:** Scenario execution (`POST /api/scenarios/run`) for `residency`, `outage`, and `quorum` returns static mock success text strings without executing real cluster perturbations. Only `slowlink` mutates in-memory link state.
   - **Defect:** Broker down scenarios cannot be cleanly separated from gateway failure because the broker is embedded inside the gateway binary.

---

### Section D: Frontend Integrity & Assets
1. **LIVE vs DEMO Mode Isolation:**
   - Controlled via `isLiveMode` in `frontend/src/context/KamishContext.tsx` and mirrored in gateway `/api/mode`.
   - **Defect:** **DEMO / Simulated data appears under LIVE mode**. The 100ms interval and 500ms telemetry sampler in `KamishContext.tsx` continuously generate synthetic write latency (`0.35 + Math.random() * 0.15`), hardcoded p99 latency (`0.42ms`), and synthetic outbox queue depth that overlay live telemetry. The initial audit ledger loads mock records.
2. **Remaining Stubs & Fake Data:**
   - `frontend/src/pages/DashboardPage.jsx`: Stat cards explicitly labeled `SIMULATED QUEUE DEPTH`, `HEALTHY DEMO REGIONS`, `ALLOWED DEMO DECISIONS`.
   - `frontend/src/context/KamishContext.tsx`: Math.random() used for background traffic jitter and write latency.
3. **3D Assets & Graphics Performance:**
   - `kamish_dragon_animated.glb` is an uncompressed **53 MB** binary; `kamish_dragon.glb` is **27 MB**.
   - Assets are loaded over HTTP without Draco/Meshopt geometry compression or streaming chunking.
   - WebGL renderer failure fallback is implemented in `KamishDragon3D.jsx` and `ThreeTopologyGlobe.tsx` (renders 2D high-contrast fallback).
   - **Defect:** **`prefers-reduced-motion` is completely ignored** in `KamishDragon3D.jsx` and `ThreeTopologyGlobe.tsx`.
4. **Accessibility (a11y) Spot Check:**
   - `SignInPage.jsx`: Form inputs have associated `<label htmlFor=...>`, `role="alert"` used for error messages, keyboard Enter key submits form.
   - `TasksPage.jsx` & `ProduceFetchPage.jsx`: Semantic headers, readable focus outlines, responsive grid layout.

---

### Section E: Honesty & Claim Analysis

A search across the codebase and documentation for restricted claims revealed:

| Audited Term | Occurrences | Documentation / Code Context | Backed by Tests? |
| --- | --- | --- | --- |
| **"zero loss"** | 2 | Explicitly listed under **Out of Scope / Non-Goals** in `project.md:115`. | Yes (Truthful disclaimer) |
| **"exactly-once"** | 7 | Explicitly disclaimed in `project.md:104,112`, `packages/kamish-client/README.md:174`, and producer docstrings ("No global exactly-once guarantee"). | Yes (Truthful disclaimer) |
| **"compliant" / "compliance"** | 127 | Repeatedly disclaimed: `worktillnow.md:6` and `project.md:19` state: "Prototype scope, not a legal-compliance assessment or compliance product." | Yes (Truthful disclaimer) |
| **"production-ready"** | 0 | Not claimed anywhere. | N/A |
| **"guaranteed"** | 10 | Used in disclaimers (e.g. "not a guaranteed exposure time") or test library typings. | Yes (Truthful disclaimer) |
| **"real-time failover"** | 0 | Not claimed. | N/A |
| **"Kafka compatible"** | 0 | Not claimed. | N/A |
| **"sealed"** | 75 | Documented in `worktillnow.md:264`, `demoTasks.js:10`, and `demoPolicy.js:21` as **disabled/unimplemented**. (Note: UI mockup in `ContactModal.jsx` lists sealed option, but engine rejects it). | Yes (Accurately documented as disabled) |
| **"end-to-end encrypted"**| 0 | Not claimed. | N/A |

---

## 4. Security Findings with Severity

| Finding ID | Severity | Description | File Location |
| --- | --- | --- | --- |
| **SEC-01** | **HIGH** | **Missing `Secure` Cookie Flag**: The session cookie `kamish_session` is set without the `Secure` attribute (`HttpOnly; SameSite=Strict; Path=/; Max-Age=3600`), allowing transmission over unencrypted HTTP. | `cmd/kamish-gateway/src/lib.rs:260` |
| **SEC-02** | **HIGH** | **Missing Cross-Tenant Isolation in Gateway**: Gateway `produce_handler` and `fetch_handler` pass `None` as client identity to `broker.dispatch_with_identity`. The authenticated `Session` has no `tenant_id`, allowing any authorized role to read or write across tenant boundaries. | `cmd/kamish-gateway/src/lib.rs:592,642` |
| **SEC-03** | **MEDIUM** | **Hardcoded Default Credentials**: Default user credentials for admin, operator, auditor, producer, and consumer are hardcoded in source code with no mechanism to override them via environment variables or secret stores. | `cmd/kamish-gateway/src/auth.rs:114-120` |
| **SEC-04** | **MEDIUM** | **Missing Security Headers**: Gateway responses omit `Content-Security-Policy` (CSP) and `Strict-Transport-Security` (HSTS), increasing exposure to clickjacking or asset injection. | `cmd/kamish-gateway/src/lib.rs:104-120` |
| **SEC-05** | **LOW** | **Custom Cryptographic Password Hashing**: Uses an in-house iterative SHA-256 loop (10,000 rounds) instead of industry-standard Argon2id or `ring::pbkdf2`. | `cmd/kamish-gateway/src/auth.rs:72-88` |

---

## 5. Ranked Gaps Against Phase 5 Acceptance Criteria

1. **Rank 1: Research Probe Desynchronization & Broken Compilation**  
   - `early_research/probes` fails to compile due to missing struct fields in `ReplicationRequest`. Probe assertions in `remaining_gaps.rs` were invalidated in prior commits rather than remaining as immutable gap detectors.
2. **Rank 2: Clippy & Formatting Violations in Core Workspace**  
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings` fails due to `clippy::too-many-arguments` in `crates/kamish-outbox/src/lib.rs:87:5`.
   - `cargo fmt --all -- --check` fails on `cmd/kamish-agent/src/lib.rs`.
3. **Rank 3: Synthetic / Hardcoded Gateway Telemetry & Verification**  
   - `/api/checkers` returns static mock JSON.
   - `/api/audit/verify` returns static PASS without validating cryptographic ledger hashes on disk.
   - `/api/scenarios/run` returns mock strings for residency, outage, and quorum without executing tests.
4. **Rank 4: Outbox Depth Metric Disconnect**  
   - Outbox depth is completely absent from the Gateway API and is generated via a synthetic in-browser ticker in `KamishContext.tsx`.
5. **Rank 5: In-Process Embedded Broker Architecture**  
   - Gateway does not connect to the broker via a decoupled TCP/TLS client; it embeds the broker in-process, preventing independent process failure or multi-node gateway deployment.
6. **Rank 6: Heavyweight 3D Assets & Missing Reduced Motion Support**  
   - 53MB uncompressed animated GLB model causes excessive initial network overhead and ignores `prefers-reduced-motion`.

---

## 6. Corrections Required for Status Documentation

### 6.1 Updates Needed in `worktillnow.md`
1. **Section 30 Status:** Update Phase 5 status from "Complete" to "Partially Complete (Functional Prototype with In-Process Embedded Broker)".
2. **Outbox Telemetry Clarification:** Explicitly state that outbox depth on the dashboard is currently simulated in the frontend context and not yet wired to `crates/kamish-outbox` through the gateway.
3. **Audit Verification Clarification:** Disclose that `/api/audit/verify` currently checks metadata checkpoint presence rather than computing a full cryptographic ledger hash chain re-verification.
4. **Probe Status:** Document that `early_research/probes` currently fails compilation against the latest `ReplicationRequest` wire struct and requires alignment.
5. **Clippy & Fmt Status:** Record the unresolved clippy warning in `kamish-outbox` and formatting diff in `kamish-agent`.

### 6.2 Updates Needed in `project.md`
1. **Phase 5 Milestone Row:** Mark Phase 5 as **PARTIAL / IN PROGRESS** rather than DONE.
2. **Architecture Diagram:** Update gateway architecture to reflect that the current gateway daemon runs an embedded in-process broker rather than connecting as an external client over mTLS.

---

*Report compiled and certified under strict read-only audit mode.*
