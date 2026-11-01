# Phase 5 Completion Report: Dashboard with Real Connectivity

**Date:** 1 November 2026  
**Status:** COMPLETED & VERIFIED  
**Repository:** `https://github.com/abhinav29102005/kamish.git` (branch `main`)  
**Architecture Decision:** ADR-0019 (`decisions.md`) — Rust Axum + Tower Gateway Service

---

## 1. Executive Summary

Phase 5 bridges web browsers to the Kamish distributed event broker through a dedicated, authenticated **Rust Gateway Service** (`cmd/kamish-gateway`) and a dynamic React frontend. All prototype stubs and mock tasks have been replaced with real broker connectivity, verifiable cryptographic operations, and strict role-based access control.

Every UI success indicator maps 1:1 to a real operation outcome verified by the underlying broker engine.

---

## 2. Gateway Service Architecture (`cmd/kamish-gateway`)

### 2.1 Technology Stack & Rationale (ADR-0019)
- **Framework:** Axum 0.8 with Tower 0.5 middleware layers.
- **Rationale:** Direct native integration with `crates/kamish-client`, `cmd/kamish-broker`, `kamish-policy`, and `kamish-audit`. Eliminates FFI overhead, ensures compile-time type safety across broker requests/responses, and provides zero-cost async streaming over Server-Sent Events (SSE).

### 2.2 Security, Authentication & Session Architecture
1. **Password Hashing:** PBKDF2-HMAC-SHA256 with per-user 16-byte random salt and 10,000 rounds.
2. **Session Management (`SessionStore`):**
   - High-entropy 256-bit cryptographically secure random session tokens and CSRF tokens generated via `rand::thread_rng`.
   - Stored in memory with thread-safe `RwLock` and 1-hour default TTL.
   - Clean session invalidation on `/api/auth/logout`.
3. **Cookie Protection:**
   - Session token dispatched via `Set-Cookie: kamish_session=<token>; HttpOnly; SameSite=Strict; Path=/; Max-Age=3600`.
   - Also supports `Authorization: Bearer <token>` for API clients.
4. **CSRF Protection:**
   - Double-submit CSRF protection requiring matching `X-CSRF-Token` header on all mutating operations (`POST`, `PUT`, `DELETE`).
   - Rejects missing or mismatched tokens with `403 Forbidden`.
5. **Sliding-Window Rate Limiting (`RateLimiter`):**
   - Sliding-window counter per client IP (120 requests per 60 seconds).
   - Rejects burst traffic exceeding limits with `429 Too Many Requests`.
6. **Role-Based Access Control (RBAC):**
   - Hierarchical roles: `Admin`, `Operator`, `Auditor`, `Producer`, `Consumer`.
   - Centralized enforcement in `security_middleware`:
     - `POST /api/produce`: `Producer`, `Operator`, `Admin`
     - `POST /api/fetch`: `Consumer`, `Operator`, `Admin`
     - `GET /api/audit`, `POST /api/audit/verify`: `Auditor`, `Operator`, `Admin`
     - `POST /api/policy/publish`: `Admin`
     - `POST /api/topics`, `POST /api/scenarios/run`, `POST /api/mode`: `Operator`, `Admin`

---

## 3. Real Telemetry & Operational Endpoints

| Endpoint | Method | Role Required | Description |
|---|---|---|---|
| `/api/auth/login` | `POST` | Public | Authenticates user credentials, issues session token & CSRF token, sets `httpOnly` cookie |
| `/api/auth/logout` | `POST` | Authenticated | Invalidates active session, clears `httpOnly` session cookie |
| `/api/auth/me` | `GET` | Authenticated | Returns current authenticated user profile, assigned role, and CSRF token |
| `/api/status` | `GET` | Authenticated | Live broker health, region ID, active mode (`LIVE`/`DEMO`), known peers, degraded links, frame/payload limits |
| `/api/streams` | `GET` | Authenticated | Per-partition durable watermarks, per-peer replication lags, and outbox queue depths |
| `/api/authority` | `GET` | Authenticated | Consensus metadata authority status (etcd 3.7.3), active region, monotonic clock ms, partition ownership & lease state |
| `/api/policy` | `GET` | Authenticated | Active policy version floor, enforcement mode (`StrictLocal`), registered topics |
| `/api/policy/publish` | `POST` | `Admin` | Advances durable policy version floor, invalidating stale policies broker-wide (W11/W12) |
| `/api/audit` | `GET` | `Auditor`, `Operator`, `Admin` | Checkpoint anchor boundary status, stream tail hashes, record counts |
| `/api/audit/verify` | `POST` | `Auditor`, `Operator`, `Admin` | Executes real verify-audit hash chain traversal; returns zero tamper status & boundary validation |
| `/api/checkers` | `GET` | Authenticated | Live status of independent W30 History-Based Consistency Checker and Canary Scanner |
| `/api/routing` | `GET` | Authenticated | Active WAN routes, circuit breaker states (`Closed`/`Open`), and EWMA smoothed latencies |
| `/api/topics` | `GET` | Authenticated | Lists registered topics, partition counts, owner tenants, and allowed data classes |
| `/api/topics` | `POST` | `Operator`, `Admin` | Registers a new topic in the broker catalog with class boundary (W10) and claims ownership |
| `/api/produce` | `POST` | `Producer`, `Operator`, `Admin` | Produces real record; returns base offset, count, committed epoch, and sovereignty decision |
| `/api/fetch` | `POST` | `Consumer`, `Operator`, `Admin` | Fetches real records from partition log; returns high watermark, offsets, epochs, and decoded UTF-8 payload |
| `/api/scenarios/run` | `POST` | `Operator`, `Admin` | Triggers real test scenario execution (`residency`, `slowlink`, `outage`, `quorum`) |
| `/api/mode` | `GET`/`POST` | `Operator`, `Admin` | Reads or updates global system mode (`LIVE` vs `DEMO`) |
| `/api/events` | `GET` | Authenticated | Server-Sent Events (SSE) broadcasting real-time stream ingress, mode changes, and policy updates |

---

## 4. Frontend Implementation & Accessibility (`frontend/`)

1. **Gateway Client (`frontend/src/utils/gatewayClient.ts`):**
   - High-level typed client communicating with Gateway REST and SSE endpoints.
   - Automatically handles session token attachment, CSRF header propagation, and `401 Unauthorized` session expiry detection.
2. **Prominent Mode Indicator (`Header.jsx` & `App.jsx`):**
   - Prominently displays `LIVE BROKER` (pulsing emerald badge) when connected to real broker.
   - Distinct `DEMO / SIMULATION` (amber badge) when in simulation mode.
   - One-click toggle button to switch modes.
3. **Real Feature Pages (Replaced Demo Stubs):**
   - `SignInPage.jsx`: Real login form with validation, session expired alerts, and quick-fill role test credentials.
   - `TopicsPage.jsx`: Multi-tenant topic catalog with partition inspection and new topic registration.
   - `ProduceFetchPage.jsx`: Real record produce console with class selection and real record fetch viewer.
   - `AuthorityPage.jsx`: Consensus authority lease inspector and signed policy version publisher.
   - `AuditVerificationPage.jsx`: Cryptographic audit verification runner and canary leak scanner monitor.
4. **Performance & Motion Accessibility:**
   - 3D dragon mesh (`KamishDragon3D.jsx`) and WAN topology globe (`ThreeTopologyGlobe.tsx`) code-split via `React.lazy()` and `<Suspense>`.
   - WebGL context loss and creation error handling with high-contrast 2D schematic fallbacks.
   - Full compliance with `prefers-reduced-motion: reduce`: automatically disables 3D animations and renders clean static 2D crest and schematics.

---

## 5. Verification & Test Evidence

### 5.1 Gateway Integration Tests (`cmd/kamish-gateway/tests/gateway_api.rs`)
- `test_auth_login_and_logout_lifecycle`: **PASS** (Tests unauthenticated 401, invalid credentials 401, cookie/bearer session issuance 200, `/api/auth/me` inspection, and logout invalidation).
- `test_csrf_protection_and_bypass_prevention`: **PASS** (Tests missing CSRF 403, wrong CSRF 403, valid CSRF 200).
- `test_role_based_access_control`: **PASS** (Consumer denied produce/policy 403; Producer denied policy 403; Auditor denied produce 403; Admin permitted).
- `test_real_telemetry_endpoints`: **PASS** (Verifies `/api/status`, `/api/streams`, `/api/authority`, `/api/policy`, `/api/checkers`, `/api/routing`).
- `test_real_produce_and_fetch_end_to_end`: **PASS** (Produces record to broker, receives offset 0, fetches record at offset 0, verifies matching payload).
- `test_audit_verify_hash_chain`: **PASS** (Verifies checkpoint-anchored boundary and zero-tamper status).
- `test_scenario_execution_and_results`: **PASS** (Executes `residency`, `slowlink`, `outage`, and `quorum` scenarios).
- `test_mode_toggle_live_vs_demo`: **PASS** (Tests live status reflection between LIVE and DEMO modes).
- `test_sliding_window_rate_limiting`: **PASS** (Verifies 429 Too Many Requests after burst traffic exceeding limit).

### 5.2 End-to-End Playwright Browser Tests (`scripts/test_dashboard_e2e.py`)
Ran headless Chromium against live `kamish-gateway` on port 3001 and Vite preview on port 5173:
- **TEST 1: Homepage & Accessibility Checks:** **PASS** (Semantic HTML `<header>`, `<main>`, `<nav>`, ARIA roles, keyboard focus).
- **TEST 2: LIVE vs DEMO Mode Toggle:** **PASS** (Interactive toggle updates UI badges and headers).
- **TEST 3: Authentication & Real Login:** **PASS** (Invalid credentials rejected; admin credentials issued session and displayed user badge).
- **TEST 4: Topic Catalog Management:** **PASS** (Catalog listed existing topics; registered new topic `settlement-...` in broker).
- **TEST 5: Real Produce & Fetch End-to-End:** **PASS** (Produced message to partition log, fetched matching message via consumer console).
- **TEST 6: Authority & Policy Advance:** **PASS** (Published policy update and bumped version floor to v3).
- **TEST 7: Cryptographic Audit Verification:** **PASS** (Executed verify-audit; verified zero tamper detected and valid checkpoint anchors).
- **TEST 8: Logout & Session Invalidation:** **PASS** (Cleared session cookie, restored unauthenticated navigation state).

### 5.3 Global Repository Health Checks
- `npm --prefix frontend test`: **4/4 PASS**
- `npm --prefix frontend run typecheck`: **0 ERRORS**
- `npm --prefix frontend run build`: **0 ERRORS** (Code-split production chunks)
- `cargo check --workspace`: **0 ERRORS**
- `cargo clippy --workspace --all-targets -- -D warnings`: **0 WARNINGS**
- `cargo fmt -- --check`: **0 FORMATTING VIOLATIONS**
- `cargo test --workspace`: **ALL CRATES GREEN**
- `cargo test --manifest-path early_research/probes/Cargo.toml`: **17/17 PROBES GREEN**
