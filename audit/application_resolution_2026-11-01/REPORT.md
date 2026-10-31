# Application fixes and synchronization — 1 November 2026

**Immediate bugs and quality gates resolved; not production-ready.**
The original evidence in `audit/application_testing_2026-11-01/` is preserved.
This directory contains the subsequent validation outputs for the fixes.

## Synchronization

The local test-report commit was rebased onto the remote formatting/Clippy commit
without conflicts or loss of either change set. The remote fixes were retained;
further implementation changes below were made on that combined working tree.
The user's updated `result.txt` is retained unchanged as supplied evidence.

## Backend fixes

- Reject stale replication epochs **before** duplicate acknowledgment.
- Compare stored epoch/class/payload/checksum before calling a retry identical.
- Reject unsupported declared ranges: current transfer payload is exactly one
  opaque record. It is not a multi-record batch encoding.
- Validate the configured target region before acknowledging transfers.
- Fix a partition's class/restriction and admitted destinations from its first
  local record. Changing that contract requires a new topic, rather than silently
  creating unfillable offset-preserving mirror gaps.
- CLI produce invocations use fresh producer IDs. This fixes independent writes;
  it is not cross-invocation idempotent recovery of an unknown outcome.
- `AuditLog::verify_file()` reads existing evidence without creating/writing the
  path. CLI verification now rejects missing evidence.
- Added in-tree regression tests and retained semantic stale/reconnect tests;
  old two-record retry fixtures now use the supported one-record wire contract.
- Formatting and strict Clippy pass without suppressing the reported lint issues.
- The demo script propagates pipeline failures and no longer prints obsolete
  test totals or blanket production guarantees.

## Frontend fixes

- Persistent global **DEMO / SIMULATED** disclosure; no live broker claims.
- Fake credential sign-in replaced with a password-free demo-access explanation.
- Contact action is explicitly a local preview, not a successful transmission.
- Demo tasks now persist bounded metadata in a separate local-storage namespace
  and appear in Tasks/Work after a fresh page load. Failed storage does not report
  success; invalid existing storage is not overwritten.
- Structured policy selections validated before simulated ingestion; malformed
  strings and unavailable sealed mode fail closed.
- Correct allowed/denied audit outcomes and stable sequence keys; missing task IDs
  show a not-found state rather than fabricated verified metadata.
- Local audit hashing uses domain-separated structured encoding and real
  WebCrypto only. No pseudo-SHA-256 fallback. Append operations are serialized.
- Local checks no longer claim hardware fsync, residency or broker verification.
- Explicit form labels, keyboard-accessible task rows/logo, and focus indicators.
- Unknown path/hash routes both produce not-found behavior.
- Pages are lazy-loaded; large graphics code is separate from the main entry.
- Removed unused imports, render-time ref mutations and incorrect effect
  dependencies. The telemetry sampler no longer resets on every 100 ms node tick.
- Simulated rates use the configured rate instead of fabricated benchmark values.
- 3D Story advertises design illustration/custom protocol, not Kafka compatibility
  or implemented consensus/zero-loss guarantees. `robots.txt` is supplied.
- Restored locked frontend dependencies; native TypeScript checking works.

## Validation

| Check | Result |
| --- | --- |
| Rust workspace tests | **102 passed** |
| Rust format / strict Clippy | PASS |
| Original remediation probes | **11 passed** |
| Extended backend probes | **4 passed, 2 remain failing** |
| Real CLI smoke | **5 contract checks passed**, including expected missing-file rejection |
| Node SDK unit/protocol tests | **23 passed** |
| Node↔Rust restart integration / package consumer | PASS |
| Frontend lint | PASS, **zero warnings** |
| Native frontend TypeScript check | PASS |
| Frontend utility/storage tests | **4 passed** |
| Browser crypto utility checks | **3 passed**; explicit unavailable-crypto rejection is accepted |
| Dependency tree / advisory checks | PASS; no missing frontend packages or reported npm advisories |
| Vite build | PASS; main entry about **247 kB**, versus 903 kB previously |

The graphics chunk remains about 636 kB and triggers a size warning. It is
lazy-loaded, not removed or hidden by increasing the warning limit. These sizes
do not establish real mobile load/performance guarantees.

Raw outputs: [checks.json](checks.json), [CLI checks](cli-smoke.json),
[crypto checks](frontend-crypto.json), and [dependency checks](dependency-checks.json).
The extended command intentionally still exits nonzero: unresolved requirements
have not been marked ignored or disguised as passing tests.

### Browser rechecks

- A locally saved synthetic task was present in a new tab and its metadata/status
  were correctly displayed in Work.
- Sign-in had **zero password fields** and no authentication-success claim.
- Contact showed the local-preview action; all fields had associated labels.
- Unknown path and unknown task both displayed not-found states.
- Two simulated events produced correct allowed/denied counters and a valid
  six-entry local hash chain, with no observed console errors.
- New Task Lighthouse scores: **100 accessibility, 100 best practices, 100 SEO**
  in the tested desktop development page. See [Lighthouse output](results/lighthouse-new-task.json).

These are scoped browser checks, not a full device matrix, security certification,
production-hosting audit or proof of backend functionality.

## Remaining requirements — explicitly unresolved

1. **Consensus-backed ownership / unknown-owner admission.** Independent regions
   still have independent metadata and an unregistered single-node compatibility
   mode. True global fencing requires durable authority and replica eligibility,
   not another in-process mutex. That extended probe remains failing.
2. **Trusted audit completeness checkpoint.** A valid prefix cannot reveal a
   removed suffix without an independently trusted anchor. Read-only verification
   fixes evidence creation but does not supply that anchor. That probe remains
   failing.

Other previously stated missing capabilities also remain: regional quorum
durability, authenticated TLS transport, a real frontend gateway/authentication
service, secure sealed replication/key custody/escrow, lifecycle deletion and
retention protocols, and long-duration/distributed failure evidence.

This pass does not claim to resolve those features through labeling or unit tests.
The npm SDK remains private/unpublished. The overall public-production release
gate remains **FAIL**, despite green local regression and quality gates.

## Re-run without overwriting original evidence

```bash
python3 audit/application_testing_2026-11-01/run_checks.py --output-dir audit/application_resolution_2026-11-01
python3 audit/application_testing_2026-11-01/run_cli_smoke.py --output-dir audit/application_resolution_2026-11-01
python3 audit/application_testing_2026-11-01/run_dependency_checks.py --output-dir audit/application_resolution_2026-11-01
KAMISH_CRYPTO_OUTPUT=audit/application_resolution_2026-11-01/frontend-crypto.json node audit/application_testing_2026-11-01/frontend_crypto_probe.mjs
```

Results were taken against the combined working tree before its resolution commit.
The base revision in the raw metadata is not a claim that the fixes existed in
that earlier commit alone.
