# Kamish Release Runbook

This document defines the formal procedures for packaging, validating, dry-running, releasing, and rolling back Kamish distribution artifacts across npm and binary releases.

> **CRITICAL RULE**: Do not publish packages, push release git tags, or upload release artifacts without explicit, written approval from the project lead.

---

## 1. Versioning & Package Topology

Kamish follows Semantic Versioning 2.0.0 (`MAJOR.MINOR.PATCH-PRERELEASE`). During pre-1.0 research validation, all releases carry an alpha pre-release tag: `0.1.0-alpha.N`.

### Distributed Packages

1. **`kamish`** (Root Global CLI Wrapper):
   - Path: `packages/kamish`
   - npm install target: `npm install -g kamish`
   - Dependencies: `@kamish/client`
   - Optional Dependencies: `@kamish/kamish-<platform>`
2. **`@kamish/kamish-<platform>`** (Platform Native Binaries):
   - `packages/kamish-linux-x64`
   - `packages/kamish-linux-arm64`
   - `packages/kamish-darwin-x64`
   - `packages/kamish-darwin-arm64`
   - `packages/kamish-win32-x64`
3. **`@kamish/client`** (Decoupled TypeScript Client SDK):
   - Path: `packages/kamish-client`
   - Pure protocol transport, zero dashboard or heavy binary dependencies.

---

## 2. Release Dry-Run Checklist

Before any release is proposed, every gate in this checklist must be verified and logged under `audit/`:

- [ ] **Repository Hygiene**: Pinned lockfiles (`Cargo.lock`, `package-lock.json`) are committed.
- [ ] **Code Formatting**: `cargo fmt --check` passes with zero diff.
- [ ] **Static Analysis**: `cargo clippy --workspace --all-targets --all-features -- -D warnings` emits zero warnings.
- [ ] **Workspace Tests**: `cargo test --workspace` passes (all unit, integration, and doc tests green).
- [ ] **Research Probes**: All 17 verification probes in `early_research/probes` pass.
- [ ] **Frontend & Dashboard**: `cd frontend && npm test && npm run build` completes with 0 errors.
- [ ] **Client SDK**: `cd packages/kamish-client && npm test && npm run build` completes.
- [ ] **Binary Compilation**: `cargo build --release -p kamish` compiles cleanly.
- [ ] **Binary Stripping & Size Audit**:
  - `strip target/release/kamish`
  - Compressed binary size verified strictly `< 35 MB` (current: **7.4 MB**).
  - Wrapper tarball size verified strictly `< 50 KB` (current: **7.4 kB**).
  - 3D heavy assets (`models/*.glb`) verified excluded from compiled binary.
- [ ] **Clean-Room Install**: `python3 scripts/test_clean_room.py` executes full end-to-end lifecycle in fresh temporary prefixes.
- [ ] **Supply Chain Audit**:
  - `NOTICE` file up to date with all third-party components and licenses.
  - Full git commit history secrets scan completed with zero detected leaks (`audit/secrets_scan_report.txt`).

---

## 3. Dry-Run Verification Commands

Execute the dry-run packaging commands:

```bash
# 1. Build and copy stripped binary
cargo build --release -p kamish
strip target/release/kamish
cp target/release/kamish packages/kamish-linux-x64/bin/kamish

# 2. Package dry-run and inspect contents
(cd packages/kamish-client && npm pack --dry-run)
(cd packages/kamish-linux-x64 && npm pack --dry-run)
(cd packages/kamish && npm pack --dry-run)

# 3. Simulate npm publish with provenance
(cd packages/kamish-client && npm publish --dry-run)
(cd packages/kamish-linux-x64 && npm publish --dry-run)
(cd packages/kamish && npm publish --dry-run)
```

Verify that:
1. No test files, temporary keys, or secret fixtures appear in the tarball file lists.
2. The file count matches the `files` allowlist in each `package.json`.

---

## 4. Execution (Gated on Explicit Approval)

When explicit written approval is received:

1. **Remove Private Guard**:
   Update `"private": false` in `packages/kamish/package.json` and platform packages.
2. **Git Tagging**:
   ```bash
   git tag -s v0.1.0-alpha.1 -m "Release v0.1.0-alpha.1"
   git push origin v0.1.0-alpha.1
   ```
3. **CI Automated Publish**:
   The protected GitHub Actions release workflow (`.github/workflows/release.yml`) triggers on the verified tag, authenticates via OIDC provenance, and publishes to the npm registry.

---

## 5. Post-Release Smoke Test

From an external, clean container or machine without access to the local repository:

```bash
# 1. Install global package from npm
npm install -g kamish@0.1.0-alpha.1

# 2. Verify installed executable
kamish --version
kamish doctor

# 3. Initialize test cluster
kamish init --admin-user smoke_admin --admin-password TestPassword2026!
kamish policy verify --policy policy.signed.json --trusted-keys admin.pub

# 4. Start daemon and test roundtrip
kamish start &
PID=$!
sleep 2
kamish status --username smoke_admin --password TestPassword2026!
kamish topic list --username smoke_admin --password TestPassword2026!
kamish produce --topic general-topic --partition 0 --class-id general --payload '{"smoke":true}'
kamish fetch --topic general-topic --partition 0 --class-id general --offset 0
kill -TERM $PID
```

---

## 6. Rollback & Deprecation Procedure

In the event of a critical defect or security vulnerability discovered post-release:

### Within 72 Hours (Zero Downstream Dependents)
If the package has zero downstream dependents on npmjs.org, unpublish within the 72-hour window:
```bash
npm unpublish kamish@0.1.0-alpha.N --force
npm unpublish @kamish/kamish-linux-x64@0.1.0-alpha.N --force
npm unpublish @kamish/client@0.1.0-alpha.N --force
```

### Post-72 Hours (Deprecation Protocol)
Mark the affected release as deprecated with an explicit security rationale and instructions:
```bash
npm deprecate kamish@0.1.0-alpha.N "Critical vulnerability detected. Upgrade immediately to v0.1.0-alpha.N+1."
npm deprecate @kamish/client@0.1.0-alpha.N "Deprecated due to protocol compatibility fix. Upgrade to v0.1.0-alpha.N+1."
```

If crates are published to crates.io:
```bash
cargo yank --vers 0.1.0-alpha.N kamish
```

---

## 7. Credential Custody & Access Control

- **Publishing Credentials**: Held strictly by the Kamish Core Maintainers.
- **Hardware 2FA / WebAuthn**: Mandatory for npm and GitHub release permissions.
- **Automated CI Publishing**: Configured via GitHub Actions OIDC (OpenID Connect) provenance tokens. Long-lived personal access tokens (PATs) or npm authentication tokens are strictly forbidden in repository secrets.
