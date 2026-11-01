# Kamish frontend — hybrid gateway console and local simulation

The React/Vite frontend contains a **hybrid** architecture supporting both live broker gateway interaction and browser-local demonstration simulation. The UI makes the provenance and connection boundaries explicit to the operator.

## Architecture & Modes

### 1. Live Broker Mode (`isLiveMode: true`)
- Connected to the Rust HTTP Gateway (`cmd/kamish-gateway`).
- Interacts with real broker endpoints:
  - Session authentication with PBKDF2-HMAC-SHA256 and CSRF tokens (`/api/auth/*`).
  - Real record produce and offset fetch (`/api/produce`, `/api/fetch`).
  - Real regional gateway health and authority state (`/api/status`, `/api/authority`).
  - Real bounded destination outbox queue depth and replication lags (`/api/streams`).
  - Aggregate accepted/denied decision counts, broker throughput, and p99 latency are **not currently exposed by the gateway API**; the dashboard displays `N/A` instead of substituting demo values.
  - Live cryptographic audit log verification (`/api/audit/verify`, `/api/audit`).
  - Reactive server-sent events (`/api/events`).
- Live cluster badge dynamically displays true gateway connection state (`CONNECTED`, `CONNECTING`, `AUTH REQUIRED`, `GATEWAY UNAVAILABLE`, or `CONFIG ERROR`), eliminating false-green indicators when the backend is offline.

### 2. Demo Simulation Mode (`isLiveMode: false`)
- Local browser simulation for demonstrations, UI previews, and chaos engineering testing.
- Exercises client-side WebCrypto SHA-256 tamper-evident hash chaining.
- Simulates WAN delays, network partitions, and failover scenarios (A, B, C) in local state.
- Clearly marked with `[DEMO]` tags, distinct from live broker telemetry.

---

## Configuration & Environment Variables

See `.env.example` for reference settings.

| Variable | Required | Default | Description |
|---|---|---|---|
| `VITE_KAMISH_GATEWAY_URL` | No (in same-origin proxy) / Yes (in separate-origin) | `""` (relative `/api`) | Explicit target gateway base URL (e.g. `https://gateway.example.com`). Must specify `http://` or `https://`. Trailing slashes are automatically normalized. |
| `VITE_ENABLE_DEV_LOGIN` | No | `false` | When `"true"`, pre-fills development test credentials and shows quick-fill role buttons on `SignInPage`. Gated off in production builds by default. |

---

## Deployment Topologies

### Topology A: Single-Origin Reverse Proxy (Recommended for Production)
The frontend static SPA build (`dist/`) and the Rust gateway (`kamish-gateway` on port 3001) are hosted behind the same domain using a reverse proxy such as Nginx or Caddy.

- Leave `VITE_KAMISH_GATEWAY_URL` unset (or empty).
- The client automatically uses same-origin relative paths (`/api/*`, `/metrics`, `/api/events`).
- Eliminates cross-origin CORS handling, preflight overhead, and cookie samesite/secure boundary issues.
- See [`nginx.conf.example`](./nginx.conf.example) for a production Nginx configuration including SSE streaming headers and SPA fallback routing.

### Topology B: Local Development Server with Proxy
During local development, the Vite dev server runs on port 5173 and automatically proxies same-origin `/api` and `/metrics` requests to `http://127.0.0.1:3001` (configured in `vite.config.ts`). The browser client uses relative URLs by default, so this topology does not depend on an implicit loopback CORS exception.

```bash
# Terminal 1: Run Rust Gateway (dev mode with loopback binding)
cargo run -p kamish-gateway -- --port 3001 --region EU_WEST --dev

# Terminal 2: Run Frontend Dev Server
cd frontend
npm ci
npm run dev
```

### Topology C: Separate-Origin Deployment
If the frontend and gateway are hosted on distinct domains (e.g., `https://console.example.com` and `https://api.example.com`):
1. Build frontend with `VITE_KAMISH_GATEWAY_URL=https://api.example.com`.
2. Start `kamish-gateway` with the frontend origin explicitly allowed:
   ```bash
   kamish-gateway --allowed-origins "https://console.example.com"
   # or via environment variable:
   export KAMISH_GATEWAY_ALLOWED_ORIGINS="https://console.example.com"
   ```
3. The gateway strictly validates the `Origin` header against this exact whitelist, emits `Access-Control-Allow-Origin` only for an exact configured match, and enables `Access-Control-Allow-Credentials: true`. No implicit loopback-origin exception or wildcard origin is used. Wildcard origins (`*`) are prohibited with credentials.

---

## Authentication & Security Boundaries

- **Production Sign-In**: In production builds, `SignInPage` renders clean empty credential inputs and requires manual authentication.
- **Dev Gating**: Quick preset buttons (`admin`, `operator`, `auditor`, `producer`, `consumer`) and default credentials are only enabled when `import.meta.env.DEV` is active or `VITE_ENABLE_DEV_LOGIN="true"` is set.
- **CSRF & Session Protection**: All state-modifying requests (`POST`, `PUT`, `DELETE`) require `x-csrf-token` matching the session cookie.
- **WebCrypto**: Browser-side cryptographic operations require HTTPS or loopback origins.

---

## Verification & Status

- Automated tests passing at verified source revision `3bab18053c9d7a7256d8e00981531cf67a815b50` (261 unit & integration tests).
- Frontend production build (`npm run build`) verifies TypeScript compilation and asset bundling.
- For complete evidence reconciliation, see [`RECONCILIATION_REPORT.md`](../audit/release_verification_20261101/RECONCILIATION_REPORT.md).
