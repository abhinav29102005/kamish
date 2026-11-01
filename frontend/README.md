# Kamish frontend — hybrid gateway console and local simulation

The React/Vite frontend contains a **hybrid** of gateway-backed operations and browser-local demonstration state. It is not accurate to describe the entire application as either fully simulated or a fully live broker console.

## Gateway-backed paths

`frontend/src/utils/gatewayClient.ts` calls the Rust HTTP gateway for authentication, status, streams, authority, policy, audit/checker information, topic operations, produce/fetch, mode changes, and server-sent events. The gateway routes are registered in `cmd/kamish-gateway/src/lib.rs`; the default local gateway port is `3001`.

## Local/demo-only paths

- Dashboard regional nodes, link metrics, telemetry defaults, and the browser audit ledger begin with local demo data.
- `produceCustomEvent` and the built-in demonstration scenarios update local UI state and do not constitute proof that the broker processed those events.
- Unauthenticated topic, authority, and audit views can fall back to illustrative demo values.
- Demo task creation stores data in this browser's `localStorage`; it does not dispatch broker events.
- The 3D illustration is not measured system evidence.

Use the UI's mode and status indicators carefully. A successful request from one page does not mean every chart, scenario, audit row, or regional status is sourced from the live broker.

## Local development

```bash
npm ci
npm run dev
npm run typecheck
npm run lint
npm test
npm run build
```

The Vite dev server defaults to port `5173`; the gateway defaults to port `3001`. The client currently defaults to `http://127.0.0.1:3001` when running on port 5173 and otherwise uses same-origin relative API paths. No deployment-configurable gateway URL or Vite proxy was found during the 2026-11-01 source audit.

## Separate frontend/backend deployment

A separately hosted frontend is **not verified by the current repository configuration**. In production, configure a same-origin reverse proxy for `/api/*` and `/metrics`, or implement an explicit gateway-origin configuration together with a strict backend CORS allowlist and a secure cross-origin session/SSE design. The gateway currently permits localhost origins only; do not use wildcard CORS with credentials.

The frontend sign-in page currently pre-fills development credentials and provides quick-fill roles. Treat these as development-only UI, not production credentials; do not deploy this behavior to a public production console without gating/removing it.

Use HTTPS (or loopback development origins) for WebCrypto. Browser-local audit entries are not compatible with or proof of the broker's region-local audit log. The application does not establish physical residency, distributed durability, or exactly-once delivery.

## Verification status

The 2026-11-01 release verification report claims 261/261 automated tests passing at source revision `3bab180`. Benchmark execution was marked `CONFIGURED_UNEXECUTED`; live external etcd/KMS/witness services and separately hosted browser-to-gateway behavior were not established by that local test count. See [the reconciliation report](../audit/release_verification_20261101/RECONCILIATION_REPORT.md).
