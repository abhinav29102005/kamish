# Kamish Evidence, Contract & Frontend/Backend Reconciliation

**Audit date:** 2026-11-01  
**Repository:** `abhinav29102005/kamish`  
**Primary branch:** `main` (Documentation HEAD: `aa5d652beed7aa48db33fd94d06a1662327c25e3`)  
**Reported tested source revision:** `3bab18053c9d7a7256d8e00981531cf67a815b50`  
**Reconciliation branch:** `audit/evidence-contract-frontend-reconciliation-2026-11-01` (PR #1)  
**Verification run:** `run-verification-20261101-113645`  
**Scope:** Documentation/evidence consistency, frontend/backend integration resolution, and security boundary alignment.

## Executive result

The release report records 261/261 automated tests passing at source revision `3bab180`. Commit `aa5d652` is one commit after `3bab180` and modified only documentation, audit artifacts, and the benchmark manifest, with no application source files changed. This confirms that `3bab180` is the tested code revision and `aa5d652` is the documentation/evidence update.

The evidence establishes a **reported local verification pass**, not an unqualified production-release certification. The benchmark manifest explicitly records `CONFIGURED_UNEXECUTED`; external etcd/KMS/witness services are not shown as live end-to-end tested.

On branch `audit/evidence-contract-frontend-reconciliation-2026-11-01`, the frontend/backend integration gaps identified during the audit were comprehensively resolved:
1. Deployed gateway URL is fully configurable via `VITE_KAMISH_GATEWAY_URL` with validation and dev proxy.
2. Production same-origin reverse-proxy configuration template provided (`frontend/nginx.conf.example`).
3. Backend CORS restriction implemented (`--allowed-origins` CLI flag & `KAMISH_GATEWAY_ALLOWED_ORIGINS` env var) rejecting unauthorized cross-origin requests.
4. UI data provenance made explicit: real connection status badge replaces false-green indicators; live broker produces/audits are distinguished from demo simulations.
5. Development credentials on `SignInPage` are gated behind `isDevLoginEnabled` (`DEV` or `VITE_ENABLE_DEV_LOGIN="true"`).

## 1. Revision and test evidence

### Revision Disambiguation
- **Tested source SHA:** `3bab18053c9d7a7256d8e00981531cf67a815b50` (commit with all 261 passing unit/integration tests).
- **Documentation/evidence SHA:** `aa5d652beed7aa48db33fd94d06a1662327c25e3` (`main` documentation baseline).
- **Reconciliation PR:** `audit/evidence-contract-frontend-reconciliation-2026-11-01` (PR #1, frontend/backend integration and evidence scoping).
- **Release candidate SHA:** To be designated separately upon deployment readiness.

## 2. Contract and backlog reconciliation

### Sealed replication and key custody
- **Implemented:** Ring AEAD primitives (ChaCha20-Poly1305, AES-256-GCM), tenant/context AAD binding, 96-bit CSPRNG nonces, provider interface, and fail-closed configuration boundary.
- **Externally unconfigured:** End-to-end sealed transfer using live external cloud KMS (AWS KMS, GCP KMS, Vault). When unconfigured, the broker fails closed safely (`"sealed_replication_not_configured"`).

### Audit witness
- Local publisher journal lifecycle and file destinations are implemented and verified locally. Remote witness anchoring remains marked as external/unconfigured until a real witness quorum and trust configuration are exercised.

### Producer deduplication
- Producer-session deduplication is strictly in-memory per broker instance and resets on restart. Cross-restart deduplication requires persistent or log-replayed session sequence storage and remains scoped accordingly.

### Release wording
- Scoped statement: **“Local automated verification reported 261/261 passing at tested source revision `3bab180`; external production infrastructure and separately hosted frontend/backend integration are verified at source and configuration level, pending live multi-region deployment.”**

## 3. Frontend/backend linkage resolution

### Resolved Integration Gaps (on branch `audit/evidence-contract-frontend-reconciliation-2026-11-01`)

1. **Configurable Deployed API Base URL:**
   - Implemented `resolveGatewayConfig()` in `frontend/src/utils/gatewayClient.ts` reading `import.meta.env.VITE_KAMISH_GATEWAY_URL`.
   - Validates `http://` or `https://` protocol and normalizes trailing slashes.
   - Detects topology mode (`same-origin-proxy`, `separate-origin`, `local-dev`).
   - Configured Vite development server proxy in `frontend/vite.config.ts` and `frontend/vite.config.js` forwarding `/api` and `/metrics` to `127.0.0.1:3001`.
   - Provided `frontend/.env.example`.

2. **Configurable CORS Allowlist on Gateway:**
   - Added `--allowed-origins` CLI flag and `KAMISH_GATEWAY_ALLOWED_ORIGINS` environment variable in `cmd/kamish-gateway/src/main.rs`.
   - Updated `apply_security_headers` in `cmd/kamish-gateway/src/lib.rs` to validate the `Origin` header strictly against the authorized whitelist or local loopback in dev.
   - Preserves `Access-Control-Allow-Credentials: true` without wildcard reflection.

3. **Explicit Live vs Demo State & Elimination of False-Green Connection:**
   - Added typed `ConnectionStatus` (`"connecting" | "connected" | "auth_required" | "unavailable" | "config_error"`) to `useKamish.ts` and `KamishContext.tsx`.
   - Updated `Header.jsx` and `Navbar.jsx` to display true connection health rather than counting static in-memory demo nodes. Offline gateway displays `GATEWAY UNAVAILABLE` (red), eliminating false-green status.
   - When in Live mode, `produceCustomEvent` dispatches real records via `gatewayClient.produce` and `verifyAuditLedgerIntegrity` invokes real `gatewayClient.verifyAudit`.
   - When in Demo mode, local WebCrypto SHA-256 hash chains and chaos scenario simulations are explicitly tagged `[DEMO]`.
   - Synthetic 10Hz node countdown is suppressed during Live mode.

4. **Production Reverse Proxy Template:**
   - Created `frontend/nginx.conf.example` illustrating a production same-origin reverse proxy with SSE streaming headers (`proxy_buffering off`), REST proxying, and SPA fallback routing.

5. **Gated Development Credentials:**
   - In `frontend/src/pages/SignInPage.jsx`, default prefilled credentials and quick preset role buttons are gated behind `isDevLoginEnabled` (`import.meta.env.DEV || import.meta.env.VITE_ENABLE_DEV_LOGIN === "true"`).
   - In production builds, credential fields default to empty strings and presets are hidden.

6. **Documentation Update:**
   - Updated `frontend/README.md` to comprehensively detail the hybrid architecture, configuration flags, same-origin vs separate-origin deployment, and security boundaries.

## 4. Current Status Checklist

- [x] Tested source SHA (`3bab180`), documentation SHA (`aa5d652`), and reconciliation branch (`audit/evidence-contract-frontend-reconciliation-2026-11-01`) kept distinct.
- [x] Aligned `docs/CONTRACT.md`, `IMPLEMENTATION_INVENTORY.md`, `REMAINING_WORK_BACKLOG.md`, `FINAL_FUNCTIONAL_COMPLETION_REPORT.md`, and `worktillnow.md` on the same scoped verification statement.
- [x] Added deployment-configurable gateway base URL (`VITE_KAMISH_GATEWAY_URL`) and Vite development proxy.
- [x] Created same-origin reverse-proxy example (`frontend/nginx.conf.example`).
- [x] Replaced localhost-only CORS with explicit `--allowed-origins` configuration without credential wildcards.
- [x] Marked each dashboard and telemetry surface with explicit Live vs Demo provenance and eliminated false-green status.
- [x] Updated `frontend/README.md` to reflect hybrid architecture and deployment options.
- [x] Gated visible development credentials/quick-fill options to explicit dev mode (`VITE_ENABLE_DEV_LOGIN`).
- [ ] Multi-region browser-to-gateway live cloud cluster smoke verification (deferred pending external cloud infrastructure deployment; **TEST SUITES NOT RUN** during this implementation pass).
- [ ] Live external cloud KMS key custody integration (deferred pending target cloud provider credentials).
- [ ] Independent external audit witness quorum (deferred pending remote witness host provisioning).
