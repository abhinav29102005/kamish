# Kamish — application test report

**Verdict: FAIL for public/production release. Existing regression tests pass,
but additional contract tests and user-flow tests expose important gaps.**

**Session date:** 1 November 2026 (IST). Raw subprocess logs use UTC.

**Tested revision:** `d914e18b4674966536a5e9e5506789b47e4348d7`.

**Final rerun environment:** Linux; Rust/Cargo 1.99.1, Node.js 22.22.0, npm 10.9.4;
Chromium in the desktop browser; frontend served at `127.0.0.1:5187`.
The initial pass used Rust 1.94.1 without formatting/Clippy commands. The toolchain
was updated during validation; the final recorded rerun independently reproduces
the user's formatting and Clippy failures with the available tools.

This was a test-only pass. Added reproducible test probes, runners and evidence;
did not fix application code, publish packages, deploy infrastructure, or commit
changes. The user's independent validation output is acknowledged below.

## 1. Result summary

| Area | Result | Evidence / interpretation |
| --- | --- | --- |
| Rust all-target check | PASS | `cargo check --workspace --all-targets` |
| Rust workspace regression suite | **99 passed** | [rust-tests.txt](results/rust-tests.txt) |
| Original remediation probes | **11 passed** | First suite in [research-probes.txt](results/research-probes.txt); sealed test passes by safe rejection |
| Added broader contract probes | **6 failed** | Independent `remaining_gaps.rs`, detailed below; not part of the default application workspace |
| Node SDK tests | **23 passed** | [sdk-tests.txt](results/sdk-tests.txt) |
| Node↔Rust broker integration | **1 passed** | Binary bytes, large u64 identity, partition concurrency, rejection and restart dedupe |
| SDK typecheck / packaged consumer | PASS | ESM exports and declaration use tested in an isolated installed tarball |
| SDK runtime dependency audit | PASS, 0 advisories | SDK has no runtime dependencies; not a security certification |
| Broker/agent/ctl help | PASS | Includes every ctl subcommand |
| Real CLI operations | **3 passed, 2 failed** | First produce, consume and watermark pass; second produce and missing-evidence verification fail their intended contracts |
| Demo script / short benchmark | Executed successfully | Simulation/helper evidence, not proof of production regional survival or throughput |
| Frontend Vite build | PASS with bundle warning | Main bundle 903.06 kB, gzip 240.87 kB |
| Frontend lint | Exit 0 with **30 warnings** | This is not a clean lint gate |
| Frontend type validation | BLOCKED / not validated | Declared TypeScript, `@types/node`, and `@types/three` missing; diagnostic SDK TS 5.9.3 build stops at missing Node types |
| Frontend dependency audit | PASS, 0 advisories | Lockfile audit; installed dependency tree still reports missing packages |
| Browser routes / visual rendering | Main routes render | Several flows are misleading or simulated, see §4 |
| Browser crypto utility probes | **1 passed, 2 failed** | Real WebCrypto known answer passes; fallback SHA-256 claim and field-boundary integrity fail |
| Formatting / strict Clippy | **FAIL, independently reproduced** | Formatting exits 1; strict Clippy exits 101 with two broker lint errors |

Machine-readable command outcomes: [checks.json](checks.json),
[cli-smoke.json](cli-smoke.json), [dependency-checks.json](dependency-checks.json),
and [frontend-crypto.json](frontend-crypto.json). Exit status alone does not
establish a semantic PASS: the missing-audit command exits zero but fails the
evidence contract; frontend lint exits zero despite warnings.
All four runner scripts correctly return exit 1 for the known failing gates;
see [runner-exits.json](runner-exits.json).

## 2. Backend contract failures

All six were reproduced using synthetic records in temporary directories by
[remaining_gaps.rs](../../early_research/probes/tests/remaining_gaps.rs).

| ID | Severity / boundary | Reproduction | Observed result | Needed correction |
| --- | --- | --- | --- | --- |
| B01 | High: replication fencing | Persist offset 0 at epoch 7, then submit offset 0 at epoch 1 | Old epoch receives a successful duplicate ACK for epoch-7 history | Validate authoritative epoch/history before duplicate acknowledgment |
| B02 | High: replica integrity | Persist `first` at offset 0, retry same offset with `different` | Receiver reports idempotent success without comparing bytes/history | Authenticate transfer identity and compare stored record/hash on duplicate retries |
| B03 | High: range contract | Submit range `[0,5)` containing one opaque payload | Receiver accepts and applies watermark 1 | Validate/decode declared record count and range; reject unsupported ranges |
| B04 | High: distributed ownership gap | Create independent legal-region brokers with independent metadata; produce to the same partition | Both acknowledge epoch 1 | Consensus-backed registered ownership, fail-closed unknown ownership and durable fencing. This deliberately exercises the prototype's documented unregistered compatibility mode |
| B05 | High: mirror/policy contract | Source offset 0 is StrictLocal; source offset 1 allows remote replication | Source acknowledges second record but destination rejects its offset-1 range because offset 0 is absent | Bind replication contract to stream/generation, or define authorized filtered/derived offsets |
| B06 | Medium: audit evidence completeness | Write two valid audit entries; remove only the final line | Reopen accepts the remaining valid prefix | Trusted checkpoint/anchor and an explicit completeness contract. This is not a SHA-256 collision or the already-fixed format-2 field-boundary bug |

Tests passing within a broker instance do not establish authority between
independent regions. No genuine global metadata quorum exists in this testbed.

### Real CLI failures

| ID | Result | Why |
| --- | --- | --- |
| B07 | Second fresh `kamish-ctl produce` to the same partition fails | CLI hardcodes producer ID 1001; each invocation starts epoch 1/sequence 0. A different second payload triggers `producer_sequence_content_conflict`. The broker correctly prevents conflicting dedupe; the CLI needs session/identity handling. |
| B08 | `verify-audit` on a nonexistent file reports a valid chain of 0 entries and exits zero | Verification calls the create/open logger API. An evidence-verification command must distinguish missing evidence from intentionally initialized empty logs and must not create the evidence it is verifying. |

Raw outputs: [cli-produce-second.txt](results/cli-produce-second.txt) and
[cli-verify-missing-evidence.txt](results/cli-verify-missing-evidence.txt).

## 3. Frontend browser coverage

Used an isolated test tab and synthetic `.invalid` form data. Checked state with
fresh snapshots after navigation and waited for route-specific content; an initial
home render before the routing effect is not counted as a direct-route failure.

| Flow | Observation |
| --- | --- |
| Home | Dragon page renders; Flame On toggles; no real commit confirmation was established |
| Dashboard | Renders; quick-produce adds local audit rows; verification checks a local browser chain |
| Tasks | Static list renders and advertises PASS statuses; submitted test task is absent |
| Work task | Details render; verification announces hardware durability without performing a hardware/broker test |
| New task | Valid form displays success and redirects; no request occurs during submission; no persisted task appears |
| Contact | Empty form is blocked by native validation; valid synthetic form displays a transmission message. Source submit handler performs no network send |
| Sign-in | Unregistered `.invalid` credentials display authentication success and redirect with zero fetch/XHR requests during submission |
| Unknown hash route | Displays `Route not found` / 404 |
| Unknown path route | `/nonexistent-path` renders Home instead of the hash route's 404 behavior |
| Direct `/tasks` path | Tasks renders after the routing effect |
| Unknown task ID | `#/work/does-not-exist` displays `[VERIFIED / PASS]` for invented fallback metadata |
| Repeated quick produce | Double-click completed; local audit verification passed for 8 blocks in the observed interleaving. Not a proof of all asynchronous audit interleavings |
| 3D Story | `/3d.html` renders without observed console errors; its claims exceed the implemented broker behavior |

Browser request instrumentation was reset before sign-in and task submissions;
both had no fetch/XHR calls during submission. An earlier contact trace contained
unrelated model/blob loads, so absence of contact delivery is based on the inspected
handler, not falsely described as a clean network trace.

## 4. Frontend findings

| ID | Priority | Finding / evidence |
| --- | --- | --- |
| F01 | High before public use | Sign-in is an unauthenticated demonstration, but says “Authentication successful”. Dashboard is accessible before login. `SignInPage.jsx:8–14` |
| F02 | High before public use | Contact says a message was transmitted although its handler only sets/reset local state. `ContactPage.jsx:9–18` |
| F03 | High | New task reports registration/dispatch without a task store or broker call; submitted title is absent from Tasks. Source catch branch also sets success on exceptions. `NewTaskPage.jsx:13–29` |
| F04 | High | Dashboard and new-task forms pass policy strings while context expects an object with `.type`; the local simulator skips strict-policy branches and treats the malformed input as permissive. No real broker egress was performed in this flow |
| F05 | Medium | Accepted audit outcomes are `AllowCleartext`/`AllowSealed`, but dashboard counts and badges test `Accept`. Valid entries display BLOCKED and DONE stays zero. React logs a missing-key warning because `entry.id` is undefined; task navigation also uses nonexistent `entry.key` |
| F06 | High: misleading evidence | Work-page verification only verifies browser audit hashes but says “Hardware fsync and residency confirmed”. It neither invokes Rust tests nor reads a broker audit log |
| F07 | High: audit semantics | Without WebCrypto, `sha256Hex()` returns a repeated pseudo-hash rather than failing; it can still be presented as SHA-256 verification. Isolated utility probe fails a SHA-256 known-answer test for the fallback |
| F08 | Medium: audit encoding | Colon-separated browser audit fields are ambiguous: topic `a:b`, source `c` hashes identically to topic `a`, source `b:c`. Reproduced with real WebCrypto, not merely the fallback |
| F09 | High: claim accuracy | Tasks advertise sealed replication as PASS while runtime sealing is deliberately disabled. 3D Story claims Apache Kafka protocol compatibility, zero-loss failover and stages 1–5 functional; the broker uses a custom JSON/TCP protocol and lacks consensus-backed durability/failover |
| F10 | Medium | Invalid task IDs display VERIFIED/PASS; unknown path and hash routes have inconsistent not-found behavior |
| F11 | Medium: accessibility | New Task's two select fields lack associated labels. Automatic Lighthouse accessibility score is 93; direct DOM checks also found unassociated labels on inputs. Clickable task/logo divs lack keyboard semantics in source |
| F12 | Medium: load/performance | 903.06 kB primary JS bundle generates a size warning. Pages should be split/lazy-loaded and 3D behavior reviewed on constrained devices; this run did not measure mobile performance |
| F13 | Medium: development quality | 30 lint warnings cover unused variables, ref access during render and effect dependencies. Declared compiler/type dependencies are not installed |
| F14 | Low in this dev setup | Lighthouse SEO score 80; `robots.txt` is invalid/missing under Vite fallback. Check the actual production hosting configuration before assigning the same SEO verdict to deployment |

### Isolated browser-crypto results

[frontend_crypto_probe.mjs](frontend_crypto_probe.mjs) transpiles only the utility
into separate VM contexts, one with WebCrypto and one without. It does **not**
disable browser security or alter application source.

- Real WebCrypto SHA-256 known answer: PASS.
- Fallback equal to real SHA-256: FAIL.
- Distinct colon-delimited field boundaries produce distinct hashes: FAIL.

## 5. Accessibility, assets and dependencies

Automatic Lighthouse scope is limited to the audited routes; it is not a manual
keyboard audit or performance benchmark:

| Route | Accessibility | Best practices | SEO | Failures |
| --- | --- | --- | --- | --- |
| Sign-in | 100 | 100 | 80 | robots.txt |
| New Task | 93 | 100 | 80 | select labels; robots.txt |

Raw results: [lighthouse-signin.json](results/lighthouse-signin.json),
[lighthouse-new-task.json](results/lighthouse-new-task.json).
Screenshots: [New Task](results/new-task.jpeg), [3D Story](results/story.jpeg).
Placeholders can give inputs an automatic accessible name; a score of 100 does
not mean visible labels are correctly associated or the login is secure.

`npm ls --depth=0` reports missing `typescript`, `@types/node`, and `@types/three`.
The frontend build transpiles without full type validation. The diagnostic run
uses the SDK's TS 5.9.3, not the frontend's declared TS 7.0.2, and stops at missing
Node definitions. No compiler-source errors beyond that environment blocker are
claimed. Both npm advisory checks reported zero findings at this run; this does
not cover authentication, application logic, crypto correctness or all supply-chain risks.

## 6. Formatting and Clippy: confirmed validation

The initial local toolchain could not run `cargo fmt` or `cargo clippy`. After
the toolchain update to Rust/Cargo 1.99.1, the final runner reproduces the user's
report from 01 Nov 2026, 00:20:17 IST:

- `cargo fmt` exits 1 with extensive formatting diffs across the recent code.
- Strict Clippy exits 101 with `type_complexity` on broker partition-operation
  state and `collapsible_if` during producer-session recovery.
- Rust tests pass in that run too.

Accordingly, **release formatting/lint gates are FAIL**, not PASS or merely
unknown. Raw independent outputs: [rust-format.txt](results/rust-format.txt) and
[rust-clippy.txt](results/rust-clippy.txt). The untracked user-owned `result.txt`
was not modified or removed.

## 7. Performance and coverage limits

The benchmark smoke uses 64 normal-link records and the harness's 100-record
degraded phase in debug mode. It executed and emitted measurements, but does not
establish sustained production throughput, fsync-controller power-loss behavior,
multi-region RPO/RTO, or comparability with Kafka/Pulsar. Demo-script labels are
static and still print an obsolete test count (“23”); the actual suite has 99.
The demo script also lacks `pipefail` around filtered test output.

Not established by this local pass:

- Physical region/transit residency and real independent regional outages.
- Multi-node local quorum durability, durable metadata consensus and globally safe
  ownership transfer; missing implementations cannot be certified through testing.
- mTLS/authorization, key escrow, HSM or working sealed-copy encryption.
- A frontend↔backend gateway: the UI is currently a simulator.
- Real mobile viewports/devices, Safari/Firefox, reduced-motion behavior, exhaustive
  keyboard/screen-reader behavior, long-duration memory/leak/soak testing.
- Every disk-full, fsync-error, corrupted-header, paused-process and retry ordering.
- Production hosting/CORS/DNS/TLS, CI matrix and exhaustive dependency security.

## 8. Fix order

1. Reject stale/conflicting/invalid replication ranges before ACK; bind streams to
   a stable policy/offset/generation contract and authoritative ownership.
2. Fix CLI producer sessions and distinguish read-only audit verification from
   log creation. Add the newly failing probes to owning-crate regression suites
   once their contract and fix are implemented.
3. Label the frontend as simulated; remove false authentication, transmission,
   crypto/hardware and compatibility claims. Correct input policy shapes and audit
   outcome/key mappings before any real backend integration.
4. Replace browser pseudo-hash fallback with fail-closed behavior and structured,
   versioned audit encoding. Fix form labels and invalid-task states.
5. Apply formatting, resolve strict Clippy warnings, restore declared frontend
   dev dependencies and enforce type/lint checks in CI.
6. Implement the remaining consensus, durability, transport and lifecycle work
   before describing the application as production-ready or publicly exposing it.

## Reproduce

```bash
python3 audit/application_testing_2026-11-01/run_checks.py
python3 audit/application_testing_2026-11-01/run_cli_smoke.py
python3 audit/application_testing_2026-11-01/run_dependency_checks.py
node audit/application_testing_2026-11-01/frontend_crypto_probe.mjs
```

The runners record all their results and exit nonzero if a check/contract fails.
The extended research command also intentionally exits nonzero until its six
contracts are fixed. Run just the original 11 remediation checks with:

```bash
cargo test --locked --manifest-path early_research/probes/Cargo.toml --target-dir target --lib
```

Browser flows were exercised through desktop browser tools and are documented
above; they are not presented as an automated browser CI suite.
