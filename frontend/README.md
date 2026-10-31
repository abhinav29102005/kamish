# Kamish frontend — local demonstration

This React/Vite application is a **simulator**, not a live operator console.
It does not authenticate users, send contact messages, or connect to the broker.
Do not enter credentials or regulated/sensitive data.

## Run

```bash
npm ci
npm run dev
npm run typecheck
npm run lint
npm test
npm run build
```

Use a modern browser with WebCrypto (HTTPS or a loopback development origin).
If WebCrypto is unavailable, audit hashing fails explicitly instead of using a
pseudo-hash. The TypeScript check validates typed components/declarations;
JavaScript page behavior is also covered by utility tests and browser checks.

## Demo behavior

- `/` and `#/home` show the dragon animation; pages are lazy-loaded.
- `#/dashboard` displays **simulated** counters and local audit entries.
- `#/new` saves a demo task to this browser's `localStorage` under
  `kamish.demo.tasks.v1`. It does not dispatch a broker event.
- `#/tasks` and `#/work/<id>` read that local task catalog. Prototype status is
  not a runtime test result; sealed replication is marked **DISABLED**.
- `#/signin` explains that authentication is unavailable and offers demo access;
  it does not collect passwords or claim authentication succeeded.
- `#/contact` previews a message locally. It does not send it.
- `/3d.html` is an animated design illustration, not measured system evidence.

Task storage is bounded and not encrypted. Invalid existing storage is not
silently overwritten. Audit events are browser-only, in memory, and are not
compatible with or proof of the broker's region-local audit log. The app does not
establish physical residency, distributed durability, or exactly-once delivery.

See [the resolution report](../audit/application_resolution_2026-11-01/REPORT.md)
for tests and remaining gaps. A real console requires an authenticated backend
gateway; the Node.js SDK cannot be imported directly as a browser TCP client.
