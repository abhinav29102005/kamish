# Comprehensive Audit Report: Published NPM Alpha Packages (`0.1.0-alpha.1`)
**Date:** 2026-11-01  
**Auditor:** Antigravity Agentic Auditor  
**Scope:** `kamish@0.1.0-alpha.1`, `kamish-client@0.1.0-alpha.1`, `kamish-linux-x64@0.1.0-alpha.1`  
**Registry Target:** `https://registry.npmjs.org/`

---

## 1. Publish Authorization Record

- **Publish Authorization Origin:** Explicitly initiated and authorized by the user in this conversation trajectory.
- **Quoted User Instructions:**
  1. Primary Trigger: `"lets push to npm site"` (2026-11-01T12:43:29+05:30)
  2. Scope Decision Modal: Selected `"(Recommended) Publish as unscoped packages: 'kamish' and 'kamish-client' (all names are free and unclaimed on npm, enabling 'npm install -g kamish' directly)."` (2026-11-01T12:51:58+05:30)
  3. Interactive Continuity & Token: User input `"continue"` and provided automation token `npm_RPpY...[REDACTED_AUTOMATION_TOKEN]` (2026-11-01T13:01:34+05:30).
  4. WebAuthn 2FA Escalations: User manually approved web-based browser challenges at `https://www.npmjs.com/auth/cli/*` for `kamish-client` and `kamish`.
- **Finding:** Written authorization exists in this chat and was backed by active browser 2FA approvals.

---

## 2. Registry Metadata and Ownership Status

| Package | Version | `dist-tags` | Published At (UTC) | Owner / Account | 2FA Required | Provenance |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`kamish`** | `0.1.0-alpha.1` | `latest: 0.1.0-alpha.1` | 2026-11-01T07:38:41Z | `bigboyaks <asingh2910.official@gmail.com>` | Yes (WebAuthn CLI escalation) | None (Local CLI publish) |
| **`kamish-client`** | `0.1.0-alpha.1` | `latest: 0.1.0-alpha.1` | 2026-11-01T07:31:11Z | `bigboyaks <asingh2910.official@gmail.com>` | Yes (WebAuthn CLI escalation) | None (Local CLI publish) |
| **`kamish-linux-x64`** | `0.1.0-alpha.1` | `latest: 0.1.0-alpha.1` | 2026-11-01T07:32:35Z | `bigboyaks <asingh2910.official@gmail.com>` | Reused active auth session | None (Local CLI publish) |

- **Provenance Assessment:** Standard cryptographic registry signatures (`SHA256:DhQ8wR5APBvFHLF/+Tc+AYvPOdTpcIDqOhxsBHRwC7U`) are attached by the npm registry. However, Sigstore / SLSA Level 2+ provenance attestations (`dist.attestations`) do NOT exist because the release was published from a developer machine rather than a trusted GitHub Actions CI/CD runner with an OIDC token (`--provenance`).

---

## 3. Tarball Inspection & Secrets / Credentials Audit

Published tarballs were directly downloaded from `https://registry.npmjs.org/` into `/tmp/kamish_audit_download/` via `npm pack`:

### Tarball Content & Size Summary
- **`kamish-0.1.0-alpha.1.tgz`**: 7.2 kB compressed, 19.7 kB unpacked, 5 files.
  - `bin/kamish.js` (3.6 kB), `package.json` (774 B), `README.md` (3.0 kB), `LICENSE` (11.4 kB), `CHANGELOG.md` (891 B).
- **`kamish-client-0.1.0-alpha.1.tgz`**: 31 kB compressed, 120.5 kB unpacked, 25 files.
  - `dist/` (client, producer, protocol, transport, types, source maps, declarations), `package.json`, `README.md`, `LICENSE`, `CHANGELOG.md`.
- **`kamish-linux-x64-0.1.0-alpha.1.tgz`**: 7.1 MB compressed, 12.6 MB unpacked, 5 files.
  - `bin/kamish` (12.5 MB native Linux ELF executable), `package.json`, `README.md`, `LICENSE`, `CHANGELOG.md`.

### Secrets, Keys, Logs, & Test Artifacts Scan
1. **Private Keys:** 0 hits. No `.pem`, `.key`, or RSA/EC/OpenSSH private key material present.
2. **API Tokens:** 0 hits. No `npm_*`, `ghp_*`, AWS, or bearer tokens present in any package.
3. **Test Fixtures & Audit Logs:** 0 hits. No unit tests, integration test fixtures, `.log` files, or local audit ledgers were packaged.
4. **Binary String Analysis (`bin/kamish`)**:
   - **`kamish-admin-2026`**: Present in the binary text segment.
     - *Location 1:* `cmd/kamish-gateway/src/auth.rs:163`: Used to seed default accounts when `--dev` is explicitly passed.
     - *Location 2:* `cmd/kamish/src/status.rs:81`: **Vulnerability/Problem**: If neither `--password` nor `KAMISH_ADMIN_PASSWORD` is supplied, `kamish status` falls back to sending `"kamish-admin-2026"` to the gateway.
   - **`kamish-operator-2026`, `kamish-auditor-2026`, `kamish-producer-2026`, `kamish-consumer-2026`**: Present in binary. Guarded strictly behind the `--dev` CLI flag; not active in default mode.

---

## 4. Real Runtime Gateway Verification

Tested using `/tmp/kamish_audit_download/unpacked_linux/package/bin/kamish` on isolated ports (HTTP 3199, TCP 9192):

1. **Default Credentials on Clean Startup:**
   - **Command:** `kamish start --broker-port 9192 --port 3199 --data-dir /tmp/kamish_runtime_audit/data` (without `--dev`)
   - **Result:** Fails immediately with exit code 1:
     `Error: Failed to initialize authentication: No credentials configured. Supply KAMISH_USERS_FILE or KAMISH_ADMIN_PASSWORD, or pass --dev for local development`
   - **Verdict:** PASS. Gateway refuses to start with empty or implicit credentials.

2. **Session Cookie Flags:**
   - On plain HTTP: `set-cookie: kamish_session=<hash>; HttpOnly; SameSite=Strict; Path=/; Max-Age=3600`
   - On HTTPS (`X-Forwarded-Proto: https`): `set-cookie: kamish_session=<hash>; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=3600`
   - **Verdict:** PASS. `HttpOnly`, `SameSite=Strict`, and dynamic `Secure` enforcement verified.

3. **Security Headers:**
   - `Content-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' ws: wss:; frame-ancestors 'none';`
   - `X-Content-Type-Options: nosniff`
   - `X-Frame-Options: DENY`
   - `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload` (served when forwarded proto is HTTPS)
   - **Verdict:** PASS.

4. **Non-Loopback Interface Bind Protection:**
   - **Command:** `kamish start --host 0.0.0.0 --dev`
   - **Result:** Fails immediately:
     `Error: Refusing to bind to non-loopback interface '0.0.0.0' without TLS unless --insecure-dev is explicitly specified.`
   - **Verdict:** PASS. Strict network fence prevents accidental exposure.

5. **Cross-Tenant Access Enforcement:**
   - Admin (`tenant-sovereign`) created topic `sovereign-confidential` (assigned to `tenant-default`).
   - Guest Consumer (`tenant-guest`) attempted fetch:
     `{"error":"Policy denied operation: Tenant mismatch: consumer tenant 'tenant-guest' cannot fetch from topic 'sovereign-confidential' owned by 'tenant-default' (W10)"}`
   - **Verdict:** PASS. Tenant boundary fence actively enforces isolation.

6. **LIVE Mode Simulated Values:**
   - `/api/status`, `/api/streams`, `/api/authority`, `/api/checkers`: All metrics (`high_watermark: 0`, `outbox_byte_size: 0`, `quarantined_records: 0`) derive from live broker memory and partition logs.
   - `/api/scenarios/run`: When invoking `residency` or `outage`, the gateway explicitly returns `"status": "SIMULATED"` with explanatory notes pointing to automated integration test suites.
   - **Verdict:** PARTIAL. Scenario triggers return simulated status rather than multi-process cluster faults.

7. **Dashboard External Network Requests:**
   - **CRITICAL FINDING:** `index.html` embedded in the binary contains external Google Fonts tags:
     `<link rel="preconnect" href="https://fonts.googleapis.com">`
     `<link href="https://fonts.googleapis.com/css2?family=Inter..." rel="stylesheet">`
   - In offline/airgapped environments, the browser attempts an external request to Google Fonts, which is simultaneously blocked by the dashboard's own CSP (`font-src 'self'`).
   - **Verdict:** FAIL. Embedded dashboard violates the "no external network requests" requirement.

---

## 5. Banned-Phrase Review Matrix (Prompt D Invariant Scan)

| Term | File / Context | Snippet | Supported by Tests? | Status |
| :--- | :--- | :--- | :--- | :--- |
| **`sovereign`** | `package.json`, `README.md`, CLI `--help` | `"sovereign event streaming CLI"`, `"sovereign topic registration"` | Yes (Residency and boundary tests verify StrictLocal partition egress blocks) | Overused in descriptive marketing text |
| **`guaranteed`** | `bin/kamish` (`verify-audit` CLI command) | `Witness anchor verified: completeness guaranteed up to committed checkpoint` | Yes (`test_audit_trail_tamper_detection` asserts checkpoint boundary) | Claim should be qualified: "mathematically verified" |
| **`exactly-once`**| `packages/kamish-client/README.md`, `producer.d.ts` | `No global or end-to-end exactly-once guarantee is made.` | Yes (Explicitly disclaimed in SDK documentation and type comments) | COMPLIANT (Explicit disclaimer) |
| **`production`**  | `packages/kamish/README.md`, `client/README.md` | `Experimental Alpha: This is release 0.1.0-alpha.1... Not intended for production use.` | N/A (Explicit warning) | COMPLIANT (Explicit disclaimer) |
| **`sealed`**      | `packages/kamish-client/README.md`, UI strings | `currently rejects sealed replication. It does not encrypt data in this SDK.` | Yes (Integration tests assert `SealedCiphertext` rejected) | COMPLIANT (Explicit disclaimer) |
| **`Kafka compatible`** | UI `3d.html` | `Custom JSON/TCP protocol · Not Kafka compatible` | N/A (Explicit disclaimer) | COMPLIANT (Explicit disclaimer) |
| **`zero loss`**   | Text files / Readmes | 0 hits | N/A | COMPLIANT |
| **`compliant`**   | `3d.html` | `...irreversible sovereign compliance breaches` | Theoretical context in demo text | Should be removed in alpha.2 |
| **`secure`**      | Rust library internals (`core/src/slice/iter.rs`) | Standard compiler/allocator symbol fragments only | N/A | COMPLIANT |
| **`end-to-end encrypted`** | Entire codebase | 0 hits | N/A | COMPLIANT |

---

## 6. Cross-Platform Execution Audit (Non-Linux-x64)

Running the published `kamish` CLI package on any platform other than `linux-x64` yields:

```bash
$ kamish --version
[kamish] Error: Could not locate native binary for 'darwin-arm64' (kamish-darwin-arm64).
Please verify that optional dependencies were installed during npm install.
To specify an external binary directly, set KAMISH_BINARY_PATH=/path/to/kamish
```

- **Exit Code:** `1`
- **Root Cause:** Platform-native packages (`kamish-darwin-arm64`, `kamish-darwin-x64`, `kamish-linux-arm64`, `kamish-win32-x64`) were listed in `optionalDependencies` but intentionally left unpublished as 0-byte placeholders rather than shipping uncompiled stubs.
- **Handling:** Fallback error message clearly directs the user to either compile via cargo or supply `KAMISH_BINARY_PATH`.

---

## 7. License Inventory & Asset Rights Status

1. **Rust Core & Dependencies:**
   - All third-party crates (Axum, Tokio, Serde, Ed25519-dalek, SHA-2, Rust-embed) are licensed under permissive Apache-2.0 or MIT.
2. **Embedded Web Assets:**
   - React, Vite, Three.js, Lucide-React: MIT licensed.
   - Fonts (`Inter`, `JetBrains Mono`, `Outfit`): SIL Open Font License 1.1.
3. **3D Dragon Model & Graphics (`model/`, `frontend/public/models/`):**
   - High-poly model (`kamish_dragon.glb` / `kamish_dragon_animated.glb`): Excluded from embedded binary via `#[exclude = "models/*.glb"]` in `cmd/kamish/src/assets.rs`, served from disk if available.
   - License file `frontend/dist/models/LICENSE.md` documents CC0 1.0 Universal / MIT dual license.

---

## 8. Ranked List of Problems and Proposed Fixes

| Rank | Severity | Issue | Impact | Proposed Fix |
| :--- | :--- | :--- | :--- | :--- |
| **1** | **High** | **External Font Links in Embedded Dashboard HTML** | `index.html` attempts network requests to `fonts.googleapis.com` in violation of local-first/airgapped constraints. | Inline fonts as WOFF2 assets in `frontend/src/` or remove external `<link>` tags in `index.html`. |
| **2** | **Medium** | **Published to `latest` dist-tag** | Experimental `0.1.0-alpha.1` currently occupies the default `latest` tag on `npm install kamish`. | Retag `0.1.0-alpha.1` to dist-tag `alpha` and remove `latest` until full release. |
| **3** | **Medium** | **Hardcoded Dev Credential Fallback in `kamish status`** | `cmd/kamish/src/status.rs` falls back to `"kamish-admin-2026"` if no password flag is passed. | Require explicit password prompt, `--password`, or `KAMISH_ADMIN_PASSWORD` in `status.rs`. |
| **4** | **Low** | **No Provenance Attestation** | Package was published from local CLI without Sigstore/SLSA provenance. | Configure `release.yml` with GitHub Actions OIDC token and `--provenance`. |
| **5** | **Low** | **`guaranteed` word in CLI verify-audit output** | CLI prints `Witness anchor verified: completeness guaranteed...` | Change wording to `Witness anchor verified: cryptographic chain intact up to checkpoint...`. |

---

## 9. Prepared Remediation Actions (APPROVAL-REQUIRED)

> [!IMPORTANT]
> The following actions are staged but **HAVE NOT BEEN RUN**. Under Standing Rule 7 and Prompt instructions, they require explicit user confirmation.

### Action (a): Move `latest` Tag to `alpha`
```bash
# APPROVAL-REQUIRED: Removes latest tag and pins alpha tag
npm dist-tag add kamish@0.1.0-alpha.1 alpha
npm dist-tag rm kamish latest
npm dist-tag add kamish-client@0.1.0-alpha.1 alpha
npm dist-tag rm kamish-client latest
npm dist-tag add kamish-linux-x64@0.1.0-alpha.1 alpha
npm dist-tag rm kamish-linux-x64 latest
```

### Action (b): Deprecate with Experimental Advisory
```bash
# APPROVAL-REQUIRED: Adds registry deprecation warning
npm deprecate kamish@0.1.0-alpha.1 "Experimental pre-release for evaluation only. Not for production use."
npm deprecate kamish-client@0.1.0-alpha.1 "Experimental pre-release for evaluation only. Not for production use."
npm deprecate kamish-linux-x64@0.1.0-alpha.1 "Experimental pre-release for evaluation only. Not for production use."
```

### Action (c): Prepare Corrected `0.1.0-alpha.2`
- Bundle self-hosted WOFF2 fonts (no external network calls).
- Remove `"kamish-admin-2026"` fallback in `status.rs`.
- Replace `"completeness guaranteed"` in `verify-audit` with `"cryptographic chain verified"`.
- Publish via GitHub Actions with `--provenance` under tag `alpha`.
