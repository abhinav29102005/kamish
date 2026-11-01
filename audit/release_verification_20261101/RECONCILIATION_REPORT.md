# Kamish Evidence, Contract & Frontend/Backend Reconciliation

**Audit date:** 2026-11-01  
**Repository:** `abhinav29102005/kamish`  
**Branch reviewed:** `main`  
**Main HEAD reviewed:** `aa5d652beed7aa48db33fd94d06a1662327c25e3`  
**Reported tested source revision:** `3bab18053c9d7a7256d8e00981531cf67a815b50`  
**Verification run:** `run-verification-20261101-113645`  
**Scope:** Documentation/evidence consistency and source-level frontend/backend linkage review. No runtime browser-to-deployed-backend check was possible from this audit.

## Executive result

The report claims 261/261 automated tests passed. Git history comparison shows `aa5d652` is one commit after `3bab180`; the diff for `aa5d652` changes documentation, audit artifacts, and the benchmark manifest, with no application source files listed. This makes it plausible that `3bab180` is the tested code revision and `aa5d652` is a documentation/evidence update. The release documents should state that distinction explicitly rather than treating the two SHAs as interchangeable.

The evidence supports a **reported local verification pass**, not an unqualified production-release certification. The benchmark manifest explicitly records `CONFIGURED_UNEXECUTED`; external etcd/KMS/witness services are not shown as live end-to-end tested. The frontend is a hybrid: some operations call the Rust gateway, while other views, regional state, audit entries, and scenarios remain browser-local/demo state.

## 1. Revision and test evidence

### Observed

- `b4313b30a4d8d5e18c70f5551bf6137055397616` is the declared starting revision in `VERIFICATION_PLAN.md`.
- The final verification report names `3bab180` as the tested revision.
- The repository's current `main` resolves to `aa5d652beed7aa48db33fd94d06a1662327c25e3`.
- GitHub commit comparison reports `aa5d652` as one commit ahead of `3bab180`. The `aa5d652` changed-file list contains status/report/inventory/backlog/benchmark artifacts and no application source files.
- The release verification directory contains `REPORT.md` and `VERIFICATION_PLAN.md`; the full raw `cargo test --workspace` transcript or a machine-readable test-results file was not found in that directory.
- GitHub combined-status lookup returned no status records for either reported revision in this audit. This is not evidence that no CI ran; it means no status evidence was available from that lookup.

### Required documentation rule

Record three distinct values in the release report:
1. **Tested source SHA:** `3bab18053c9d7a7256d8e00981531cf67a815b50`.
2. **Documentation/evidence SHA:** `aa5d652beed7aa48db33fd94d06a1662327c25e3`.
3. **Release candidate SHA:** the exact SHA proposed for release, which must be confirmed separately.

Preserve raw command output, exit codes, toolchain versions, and per-command test totals in the audit directory. The 261 count is internally consistent with the published component table, but the summary table alone is not a reproducible command transcript.

## 2. Contract and backlog reconciliation

### Sealed replication and key custody

The cryptographic crate is reported to implement ChaCha20-Poly1305/AES-256-GCM and an external KMS provider boundary. The contract also says sealed transfer remains disabled, and the inventory calls the feature externally unconfigured. These claims can coexist only if they are worded as separate layers:

- **Implemented:** AEAD primitives, tenant/context binding, provider interface, and fail-closed configuration boundary.
- **Not yet operationally enabled/verified:** end-to-end sealed transfer using a live, authorized KMS/key-custody service.

Do not describe end-to-end sealed replication as fully verified until the live key unwrap/authorization path is exercised.

### Audit witness

A local durable publisher journal and file destination do not by themselves establish an independent external witness. Mark local publisher lifecycle as tested if supported by the test logs; mark independent remote anchoring as externally unconfigured until a real witness endpoint and trust configuration are exercised.

### Producer deduplication

The inventory records that producer-session deduplication uses volatile in-memory state and resets on restart. The release contract must either:
- explicitly scope deduplication guarantees to the lifetime of a broker process/session, or
- keep durable cross-restart deduplication as an open requirement.

Do not state a stronger restart-safe idempotency guarantee than the implementation supports.

### Release wording

Replace unqualified “RELEASE READY” language with a scoped statement: **“Local automated verification reported 261/261 passing at tested source revision `3bab180`; external production infrastructure and separately hosted frontend/backend integration are not verified by this campaign.”** This preserves the successful test result without implying live multi-region deployment or external key custody has been proven.

## 3. Frontend/backend linkage findings

### Positive evidence

- `frontend/src/utils/gatewayClient.ts` implements a real REST client for login, status, streams, authority, policy, audit, topics, produce/fetch, mode, and SSE events.
- `cmd/kamish-gateway/src/lib.rs` registers corresponding `/api/*` routes, including `/api/quorum`, `/api/consumer-groups`, and `/api/retention`.
- The gateway defaults to HTTP port 3001, matching the frontend's local-development fallback when the Vite dev server runs on port 5173.

### Concrete integration gaps

1. **No configurable deployed API base URL:** `GatewayClient` uses `http://127.0.0.1:3001` only when the page port is 5173; otherwise it uses an empty base URL, causing relative `/api/*` calls to target the frontend's own origin. No `VITE_*` gateway URL configuration or Vite proxy was found in the inspected files. A separately hosted frontend therefore needs a same-origin reverse proxy or an explicit configurable API origin.
2. **CORS is localhost-only:** `apply_security_headers` echoes the request origin only for `http://localhost:*` and `http://127.0.0.1:*`; other origins fall back to `http://localhost:5173`. A frontend hosted on a distinct HTTPS domain will not receive a matching `Access-Control-Allow-Origin` header. This conflicts with the current client behavior unless deployment uses a same-origin proxy.
3. **Hybrid live/demo state:** `KamishContext.tsx` initializes regional nodes, link metrics, telemetry, and browser-local audit records with static values. The dashboard reads this local state. `produceCustomEvent` updates local state/audit records instead of invoking `gatewayClient.produce`, and demo scenario handlers synthesize results. Other pages, such as Produce & Fetch, call the gateway client. Therefore, the UI must clearly label which panels are live backend data and which are simulation/demo data; the presence of some live calls does not make the whole dashboard live.
4. **Stale frontend README:** `frontend/README.md` says the application does not connect to the broker or authenticate, but the source contains a gateway client and login/session flow. Conversely, the README's simulator warning remains relevant to local-only dashboard state. Rewrite the README to describe the hybrid behavior accurately.
5. **Development credentials in the sign-in UI:** `SignInPage.jsx` pre-fills `admin` / `kamish-admin-2026` and exposes role quick-fill values. The gateway's dev-default option is off by default, but these UI values should be gated to explicit development/demo mode and never presented as production credentials.

### Integration status

**Source-level status: PARTIAL / CONFIGURATION-DEPENDENT.** The gateway client and routes exist, but separately hosted deployment is not wired by the inspected configuration, and multiple dashboard surfaces remain local/demo state. No live browser session against a deployed gateway was executed in this audit.

## 4. Required follow-up before production claims

- [ ] Attach raw test logs and toolchain/command metadata for the 261-test campaign.
- [ ] Keep tested source SHA, documentation SHA, and release candidate SHA distinct.
- [ ] Align `docs/CONTRACT.md`, `IMPLEMENTATION_INVENTORY.md`, `REMAINING_WORK_BACKLOG.md`, `FINAL_FUNCTIONAL_COMPLETION_REPORT.md`, and `worktillnow.md` on the same scoped verification statement.
- [ ] Add a deployment-configurable gateway base URL (or document and configure a same-origin reverse proxy).
- [ ] Replace localhost-only CORS behavior with an explicit deployment origin allowlist; do not use wildcard CORS with credentials.
- [ ] Mark each dashboard data source as live API, browser-local state, or demo simulation; prevent demo scenarios from being mistaken for broker evidence.
- [ ] Update `frontend/README.md` to reflect the hybrid implementation and its limitations.
- [ ] Gate visible development credentials/quick-fill options to development mode.
- [ ] Run browser-to-gateway smoke verification against the intended deployment and record the exact frontend URL, gateway URL, API status, auth/CSRF behavior, SSE behavior, and result. This remains **not run** in this audit.
- [ ] Keep external etcd, live KMS unwrap, external witness anchoring, and real benchmark execution marked as external/unexecuted until evidence exists.

## Evidence limitations

This report is based on repository source and committed documentation available through GitHub at the reviewed revision. It did not execute Rust tests, launch the gateway, access a deployed frontend, or validate external infrastructure. Source-level findings should be confirmed in the working tree before merging any code fix.
