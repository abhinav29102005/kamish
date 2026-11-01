---
title: "Kamish — Agent Prompt Pack (post Phase 5)"
version: "1.0"
date: "2026-11-01"
contents:
  - "Standing rules (prepend to every prompt)"
  - "Prompt A: Complete Phases 1-4 and re-integrate the gateway"
  - "Prompt B: Phase 6 Packaging and global install"
  - "Prompt C: Phase 6b VS Code extension (.vsix), optional"
  - "Prompt D: Phase 7 Honesty pass and release readiness"
---

# Kamish — Agent Prompt Pack (post Phase 5)

Use in this order: **Phase 5 remediation** (already issued) → **Prompt A** → **Prompt B** → **Prompt C (optional)** → **Prompt D**. Paste the **Standing Rules** at the top of each prompt, or once at the start of a long session. Each prompt ends with a stop gate. Do not start the next prompt until the gate is met and you have reviewed the report.

---

## Standing Rules (prepend to every prompt)

````text
STANDING RULES (apply to everything below)
1. Never fake closure. Do not delete, ignore, weaken, or reclassify a failing test, probe, or requirement. A probe turns green only when the behavior exists. Show me any probe-file diff before committing.
2. Evidence over assertion. Every status claim needs a commit, a test name, and raw command output saved under audit/<phase>_<item>_<date>/. Never paste summarized output as if it were raw.
3. Use the exact requirement IDs and meanings in project.md (FR-xx, NFR-xx, INV-xx) and the W-numbers in worktillnow.md. Do not relabel them. If a number and a meaning disagree with project.md, project.md wins.
4. Scope every claim: mechanism, conditions, and what was tested. Forbidden unless proven for the exact scope: zero loss, exactly-once, compliant, production-ready, guaranteed, real-time failover, Kafka compatible, end-to-end encrypted, sealed.
5. Reuse vetted libraries for consensus, TLS (rustls), AEAD, password hashing, and sessions. Never write your own crypto or consensus. Before adding a dependency, report its license, maintenance status, and fit, then wait for my pick when it is a major choice. Pin versions, record licenses, commit lockfiles.
6. One work item per commit, tests in the same commit. Never force-push or rewrite published history. Never commit secrets, keys, or tokens.
7. NEVER publish to npm, crates.io, the VS Code Marketplace, Open VSX, a container registry, or create tags and releases without my explicit written approval in this chat. Prepare, dry-run, then stop and ask.
8. If a design or security decision is ambiguous, write the options and trade-offs into decisions.md and ask me. Do not guess on consistency, security, or licensing.
9. Report each work item in this format, then stop and wait for my go-ahead:
   (1) Item and files changed   (2) Tests added, exact commands, real output (saved under audit/)
   (3) What is still not covered, stated plainly   (4) Updated FR/NFR/INV/W status
10. Be adversarial toward your own earlier claims. If you discover an earlier report was wrong, say so first.
````

---

## Prompt A — Complete Phases 1–4 and re-integrate the gateway

````text
ROLE
You are the lead engineer finishing the core of Kamish (experimental multi-region event broker with signed per-class residency policy). Phase 5 (dashboard) exists but is PARTIAL because Phases 1-4 are incomplete. Your job is to build the missing foundation, then make the gateway and dashboard show real data from it.

READ FIRST (in order)
worktillnow.md, project.md, docs/CONTRACT.md, decisions.md (ADR-0015, ADR-0016, ADR-0017), the Phase 5 audit under audit/phase5_verification_20261101/, and the latest reports under audit/.

GATE G0 - PRECONDITIONS (stop if not met)
a) The Phase 5 remediation steps 0-5 are complete and re-audited: fmt/clippy/tests green, probes compile and show 11 lib passes and remaining_gaps 4 passed / 2 failed with unchanged test names, no simulated value under LIVE, no false claims.
b) Local repo is merged with origin/main and clean.
If not met, report exactly what is missing and stop.

DECISIONS ALREADY MADE
- Metadata authority: external etcd 3.7.3 (pinned, checksum-verified official binary), 3 members, one per emulated region, mTLS, behind the MetadataAuthority trait. OpenRaft may replace it later behind the same trait. raft-rs rejected.
- Acknowledgment contract: three in-region replicas, ACK after two persist (INV-2/INV-3), as in docs/CONTRACT.md.
- Writes use time-limited leases validated on a local monotonic clock; the quorum is never consulted per write (NFR-01). Fencing is enforced by epoch at the log and replicas, never by the lease alone.
- Revocation bound = max(revocation commit + barrier ack, lease TTL + max clock drift). No watches for revocation.
- Audit: signed (tail hash, count) checkpoints in the quorum, 128-entry configurable window, local appends continue past the cap, alert when the witness is unreachable, measured time-to-anchor.

WORK PLAN (each numbered group ends with a stop gate; report in the standing format)

PART 1 - AUTHORITY (Phase 1, items 1.2-1.5)
1.2 EtcdMetadataAuthority. Use etcd-client (pure-Rust TLS feature) only after you verify its version, license, tonic/prost/tokio/rustls versions, and test against real etcd 3.7.3: Txn, lease grant/keepalive/revoke. Design rules: ownership key holds (owner_region, epoch, grant_id); a separate never-regressing epoch counter not attached to any lease; epoch CAS is a single Txn; ambiguous commit outcomes are resolved by re-reading before retry. Test harness: scripts that start/stop 3 members with generated test CA and certs (no secrets committed). Etcd tests live in a separate target gated by an env var and fail loudly in CI when etcd is missing; plain cargo test --workspace keeps working and says so.
    Required tests: two claimants race for epoch 1 (exactly one wins, many repetitions); epoch never regresses after expiry; lease expiry under paused client agrees with the local monotonic clock; keepalive loss stops ACKing; kill one member (works), kill two (metadata writes fail closed; leased local writes continue until local lease expiry); member restart and whole-cluster cold restart preserve fences; client retry after leader change.
1.3 Two-broker ownership test (W01/W02/W05). Build the real two-broker test against etcd. Show me the exact diff to independent_regions_must_not_both_accept_unregistered_epoch_one_writes BEFORE committing; the assertion stays unchanged; only the fixture may change to use the real authority. Remove or restrict set_ownership so MultiRegion cannot bypass epoch CAS (test it).
1.4 Audit witness (W08). Implement signed checkpoint commits, the 128-entry configurable window, outage metric and alert, time-to-anchor measurement, and verify-audit that reports completeness up to the last anchored checkpoint and lists the unanchored tail. In the SAME commit as the witness, update the audit probe fixture only (not its tamper assertion) and add: negative control (tail removal undetected without a witness), tamper inside the anchored prefix detected, tamper in the unanchored window documented and asserted undetectable, untouched positive control. Show me the probe diff first. State the threat-model limit: this protects against file edits, not a compromised broker, and requires the witness in a separate failure domain.
1.5 Revocation barrier. Implement the bound above, with tests for connected nodes, disconnected lease holders, and in-flight sends. Document that lease TTL bounds both revocation latency and failover time, and measure both.
GATE 1: both probes pass because of real behavior; NFR-01 outage test passes (control plane lost, pinned class keeps writing under valid lease); evidence saved. Stop.

PART 2 - SECURE AND SUPPORTED OPERATION (Phase 2)
2.1 Gateway/broker architecture. Write decisions.md options for separating the broker from the gateway process (separate broker with an authenticated client, embedded only in dev mode) and wait for my choice. Implement the choice, including failure-isolation tests (gateway crash does not kill the broker; broker crash is reported, not hidden).
2.2 mTLS 1.3 (W09) with rustls for client-broker, broker-agent, broker-gateway, and broker-metadata. Per-client identity and roles. Unknown peers rejected. Scan test traffic for plaintext.
2.3 Tenant and topic catalog (W10); producers cannot choose another tenant's class. Consumer fetch bound to authenticated identity, region and role (W13).
2.4 Policy bootstrap (W11/W12): daemons load a verified policy and trusted-key set at startup; durable policy-version floor survives restart; key rotation and revocation protocol; tampered, replayed, expired and rolled-back policy tests.
2.5 Supported deployment (W14): one defined writer per log; a test that a second process opening the same log is rejected. Server-side limits on frame, payload, connections, and request rates. A real TOML config loader so ordinary daemons run without the test fixture.
GATE 2: no plaintext inter-component traffic in the test matrix; role and tenant denial tests; policy floor survives restart. Stop.

PART 3 - DURABLE REPLICATION (Phase 3)
3.1 Segmented log (W15): rolling segment files, sparse offset index, per-record CRC, bounded recovery proportional to the last segment, torn-tail truncation, retention hooks. Recovery time and memory are measured and bounded; add tests with large logs.
3.2 Three in-region replicas (INV-2/INV-3), ACK after two fsyncs. Leader election within the region. Slow follower is removed from the ACK set and catches up. State the loss bound for whole-region loss in the docs (acknowledged records can be lost under local-quorum if not yet replicated cross-region).
3.3 Crash-window and fsync-failure tests (W17/W18): kill points around append, replicate, enqueue, ack, cursor trim; partial frames; dirty kill; fsync error handling (a failed fsync must fail-stop, never be retried as success). Durable cursors and intent policy.
3.4 Outbox and cross-region (W06/W07/W16): disk-accounted outbox with per-class budgets and fair catch-up (no unbounded memory); source identity and generation issued by the quorum; full source-envelope transfer; prefix proof across restart and retry.
3.5 Class degradation (W19): block, reject, and degrade-local behaviors per class with tests, proving other classes keep running.
3.6 Promotion (W04/W20): promotion requires a verified replica frontier and policy eligibility; common-prefix reconciliation; atomic quarantine of the superseded suffix; session and cursor state rebuild; failover tests that count duplicates and verify the checkpoint-interval bound.
3.7 Consumer groups (W22): durable checkpoints with epoch.
3.8 Rust SDK parity with the Node SDK (W21): per-stream sequences, uncertain-write retry, session journal.
GATE 3: crash/kill matrix passes with checkers; INV-2/INV-3 evidence; no gaps for data the new owner holds; bounded memory under a long outage.

PART 4 - EVIDENCE (Phase 4)
4.1 Fault injection (W31): tc/netem or network namespaces on the actual transport; delay, jitter, loss, bandwidth, partition, clock skew; calibrate achieved vs configured and publish the calibration. Fix the benchmark delay wiring.
4.2 Checkers and scanner (W30): history-based loss/duplicate/order/stale-epoch checker built from independent producer-ACK and consumer-read logs; canary scanner that decodes real stored and wire bytes, with a planted-leak positive control; epoch auditor.
4.3 Routing measurements (W26/W27): windowed smoothing of RTT, variance, loss, goodput, backlog age; circuit breaker and dwell; fair bounded retry. Report cases with and without an alternate legal path; no routing-gain claim where none exists.
4.4 Baselines (W32): version-pinned Kafka + MirrorMaker 2 and Pulsar geo-replication on the same emulated topology; record every setting and every non-normalizable difference; do not tune them less than Kamish; smoke test and lag measurement before comparisons.
4.5 Scenario runner: one command (starts etcd members, regions, load, faults, checkers, scanner) that reproduces a run from its recorded config; fixed seeds; repeated runs; medians and spread; raw logs retained; results log linking every number to a run, config and date. Implement scenarios S1-S15 from project.md as far as the built features allow; mark the rest NOT RUN with the reason.
GATE 4: a fresh clone reproduces at least S1, S2, S4, S6, S7, S8, S9, S12, S13 with one command; the results table has scope and limitations beside every number.

PART 5 - RE-INTEGRATE THE GATEWAY AND DASHBOARD
5.1 Gateway reads real quorum-backed ownership, epoch, lease, and policy floor; real outbox depth, byte size, and oldest age; real verify-audit with the anchored boundary; real checker and canary results from stored runs; real scenario-run triggers executing the runner (or returning NOT_IMPLEMENTED).
5.2 Frontend: remove any remaining simulated generators from LIVE; every number shows its source and freshness; unavailable values display "unavailable".
5.3 Re-run the Phase 5 audit checklist and Playwright E2E; correct worktillnow.md and project.md.
GATE 5: report. Stop. Do not start packaging.

DEFINITION OF DONE FOR THIS PROMPT
All fmt/clippy/tests green with etcd available; both original failing probes pass because of real behavior; every claim in the docs maps to a test; results reproducible from a fresh clone.
````

---

## Prompt B — Phase 6: Packaging and global install

````text
ROLE
You are the release engineer for Kamish. Prepare installable, verifiable packages and a global CLI. You PREPARE and DRY-RUN only. You publish nothing.

PRECONDITION GATE
Prompt A GATE 5 is met (or I have told you in writing which parts are intentionally excluded). Re-run all quality gates and report the commit. If gates fail, stop.

DELIVERABLES

1. ALL-IN-ONE PACKAGING LAYOUT (ADR-006 approved architecture)
   1. Primary package "kamish": a single global install (npm install -g kamish) providing one command "kamish". Subcommands as already specified, plus: kamish start (broker + gateway + dashboard in one process), kamish doctor, kamish init (first-run setup that creates an admin account interactively or from a secret file; no default credentials ship).
   2. One Rust executable per platform containing broker, agent, and gateway, with the built dashboard embedded (rust-embed or equivalent). Report the binary size and the embed approach, and verify the embedded static files are served with correct caching, CSP, and no external network requests.
   3. Per-platform npm packages as optionalDependencies selected by os/cpu: linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64. Built in CI from a pinned toolchain. No install-time downloads without a verified checksum. Works offline from the tarball.
   4. @kamish/client stays a separate, small SDK package. The "kamish" package may depend on it; the SDK must not depend on the dashboard.
   5. etcd is NOT bundled. kamish doctor detects it and gives actionable instructions. Provide a Docker Compose file and a checksum-verified download script for the 3-member test cluster. Single-node mode must work with no etcd and say clearly what it does not provide.
   6. Fallback distribution: GitHub Releases artifacts with SHA-256 checksums; optional Docker image built from the same binary. Do not publish either.
   7. Size and assets: set a size budget for the main tarball and each platform package. The 53 MB 3D models must be compressed or made an optional lazy download, with a lightweight default and a recorded license for every asset. If the budget is exceeded, report it and stop.
   8. Dependency control: pin all JS and Rust dependencies, commit lockfiles, run license and vulnerability scans, and list every bundled third-party component in a NOTICE file.
   9. Clean-room test: install the tarballs into a fresh container or temp prefix on Linux, macOS and Windows; run kamish --version, doctor, init, start; open the dashboard; log in; produce and fetch a record; stop; uninstall. Save transcripts.
   10. Publishing stays disabled: no npm publish, tags, or registry pushes without my explicit written approval. Check scope and package-name availability read-only and report.

2. CLI SPECIFICATION (implement and test)
   Commands: kamish --help, --version; kamish init (writes a config skeleton and a dev policy, never overwriting existing files without --force); kamish start (broker + gateway in dev mode, with the explicit --insecure-dev flag if TLS is absent); kamish status; kamish topic create|list; kamish produce; kamish consume|fetch; kamish policy sign|verify|publish; kamish verify-audit; kamish doctor (checks versions, ports, etcd availability, certificates, config validity, prints actionable errors); kamish demo (runs the labelled demo with REAL vs SIMULATED markers).
   Requirements: stable exit codes; machine-readable --json output; no secrets in logs or error output; config discovery order documented (flag > env > project file > user file); respects NO_COLOR; handles SIGINT/SIGTERM cleanly; Windows, macOS and Linux path handling; shell completions for bash, zsh, fish, PowerShell; telemetry OFF and no network calls except those the user requested. Error messages say what happened and what to do.

3. RUST BINARY DISTRIBUTION
   Preferred: per-platform npm packages as optionalDependencies (linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64) selected by os/cpu, built in CI from a pinned toolchain (rust-toolchain.toml, rust-version in Cargo.toml), with SHA-256 checksums and provenance. Fallback: GitHub Releases artifacts with a verified download step. Rules: no postinstall script that downloads code without checksum verification; no unreviewed network code at install time; installs must work offline from the tarball when the platform package is present; binaries report their version and git commit.

4. PACKAGE QUALITY
   For each package: strict "files" allowlist; README with a limitations section (what is experimental, what is simulated, what is not claimed); CHANGELOG; LICENSE; "engines" (Node >= 22.12); "type": "module"; exports map with types; "sideEffects"; provenance-ready publish config; semantic version with the alpha tag (0.1.0-alpha.N); remove "private": true only after I approve. No test fixtures, keys, audit logs, or large assets in tarballs. Verify with npm pack --dry-run and inspect the file list and size.

5. CLEAN-ROOM INSTALL VERIFICATION (the key test)
   In fresh containers or temp prefixes for Linux, and in CI runners for macOS and Windows:
   - npm pack each package; install the tarballs into a clean temp project; typecheck an import of the SDK.
   - npm install -g from the tarball into a temp prefix; run kamish --help, kamish --version, kamish doctor, kamish init, kamish start, kamish status, one produce and fetch, kamish verify-audit, then uninstall and confirm nothing is left behind except user data.
   - Install on a machine without Rust, Docker, or etcd; the CLI must give a clear message for missing optional components (for example etcd for multi-region mode).
   - Upgrade path: install alpha.1, create data, install alpha.2; data directory format version check with a clear migration error if incompatible.

6. SUPPLY-CHAIN AND LICENSING
   - Root LICENSE and NOTICE; a license inventory for Rust crates and npm dependencies (cargo-deny or equivalent, npm audit, license-checker); fail CI on disallowed licenses.
   - Resolve rights for every asset (3D models, textures, images, fonts): record source and license or replace them. Compress GLB assets (Draco/meshopt/KTX2 as appropriate) or use Git LFS; the dashboard bundle must lazy-load them.
   - Pin GitHub Actions by commit SHA; least-privilege workflow permissions; npm publish only from a protected release workflow with OIDC provenance; require 2FA; generate an SBOM; sign release artifacts if tooling is available; record checksums in the release notes.
   - Secrets scan (gitleaks or equivalent) over the full history, with a report.

7. REPOSITORY HYGIENE
   Identify active versus legacy frontend entries and duplicate configs (preserve content; propose removals, wait for approval). Track Cargo.lock and package-lock.json. Add CONTRIBUTING, SECURITY.md (how to report), and CODE_OF_CONDUCT if I confirm. Add .github workflows: fmt, clippy, tests, probes, SDK, gateway, frontend, Playwright, packaging dry-run on the Linux/macOS/Windows matrix, and an etcd-enabled job.

8. RELEASE RUNBOOK
   docs/RELEASE.md: version bump, changelog, tag rules, dry-run checklist (npm publish --dry-run for each package, provenance verification, artifact checksum verification), rollback and deprecation procedure (npm deprecate, yanking), who holds publish credentials, and a post-release smoke test from the registry. Stop before any real publish and ask me.

ACCEPTANCE CRITERIA
- All CI jobs green on the platform matrix with real logs saved.
- Clean-room install transcript for each platform attached.
- Tarball file lists and sizes recorded; no secrets or fixtures inside.
- License inventory and asset-rights table complete with no unknowns.
- docs/RELEASE.md complete, with dry-run output.
Report per work item in the standing format. Final gate: present a release-candidate checklist and wait for my written approval before any publish action.
````

---

## Prompt C — Phase 6b: VS Code extension (.vsix), optional

````text
ROLE
You are building a VS Code extension for Kamish as a thin client over the CLI and gateway. Prepare a local .vsix; do not publish to the Marketplace or Open VSX without my written approval.

PRECONDITION GATE
The kamish CLI (Prompt B) and the gateway read API exist and pass their tests. If not, stop and say what is missing.

SCOPE
1. Policy authoring: JSON schema and completions for class and policy files; diagnostics for missing legal set, unknown destination mode, invalid failover candidates, expired or stale policies; commands "Kamish: Validate policy", "Kamish: Sign policy", "Kamish: Verify policy" that call the kamish CLI (never reimplement signing).
2. Status view (tree view and status bar): broker, ownership/epoch/lease (labelled QUORUM-BACKED or LOCAL), lag, outbox depth, policy freshness, from the gateway's read-only API. Show a LIVE or DEMO or DISCONNECTED badge; never show invented values; unavailable values display "unavailable".
3. Commands: produce test record, fetch from offset, run verify-audit (shows anchored boundary and unanchored tail), run canary/checker results from stored runs.
4. Output channel with structured logs; no secrets in output.

SECURITY AND QUALITY RULES
- Authenticate through the gateway/CLI; no raw broker TCP from the extension; store tokens only in VS Code SecretStorage; never in settings or workspace files.
- Respect Workspace Trust: disable running commands that execute binaries in untrusted workspaces; declare untrustedWorkspaces capabilities properly.
- Minimal activation events (no activate on *); declare the smallest permissions and dependencies; bundle with esbuild; no remote code loading; CSP on any webview (preferably no webview).
- Telemetry off; no network calls except to the configured gateway.
- Accessibility: all UI reachable by keyboard, with labels.
- Cross-platform CLI discovery and clear errors if kamish is missing.

TESTS
Unit tests for the diagnostics and schema; integration tests with @vscode/test-electron against a real local gateway; vsce package produces the .vsix; install it in a clean VS Code profile and run the commands; verify the .vsix contents list and size; license and README with a limitations section; CHANGELOG; icon asset with a recorded license.
Deliverables: packaged .vsix, test logs under audit/, install transcript, and docs/VSCODE.md. Stop before any publish.
````

---

## Prompt D — Phase 7: Honesty pass and release readiness

````text
ROLE
You are an independent verifier. Your job is to make every public statement about Kamish true, scoped, and traceable, and to decide, with evidence, whether the project is ready for its intended audience. You may edit documentation, UI text, and READMEs. You may NOT change behavior code or tests to make a claim pass.

PRECONDITION GATE
Prompts A and B are complete (C if used). Save the exact commit hash and re-run every gate from a clean checkout of that commit.

TASKS

1. CLAIMS REGISTER (docs/CLAIMS.md)
   Build a table of EVERY externally visible claim from: README, package READMEs, docs, project.md, worktillnow.md, frontend and gateway UI text, CLI help text, the slide deck, the demo script, and the VS Code extension text if present. For each claim: ID, exact wording, source file and line, scope and mechanism, supporting test name or run ID, result, and a verdict (SUPPORTED, PARTIAL, UNSUPPORTED). Rewrite or remove every PARTIAL and UNSUPPORTED claim.
   Run an automated search for these words and justify or remove each hit: zero loss, lossless, exactly-once, compliant, compliance, certified, production-ready, enterprise-ready, guaranteed, real-time, instant failover, Kafka compatible, drop-in, end-to-end encrypted, sealed, unbreakable, secure (unqualified), proven.
   Add a CI check that fails the build when a banned phrase appears outside docs/CLAIMS.md or an explicit "what we do not claim" section.

2. TRACEABILITY MATRIX (docs/TRACEABILITY.md)
   For each FR, NFR and INV in project.md and each W item in worktillnow.md: status (Implemented-Tested, Implemented-Partial, Designed-Only, Not-Implemented, Contained/Disabled), the test names, the run IDs, commit hashes, and known limits. Use the exact project.md meanings. Flag any requirement whose evidence is only a unit test but whose claim is distributed or multi-process.

3. NOVELTY AND PRIOR-ART RE-CHECK
   Re-verify every "others do X" statement in project.md and docs against current official documentation, recording product version and access date. Known corrections to apply: Pulsar supports server-side allowed-cluster controls; ciphertext storage and offset-preserving mirroring are prior art. Rewrite the novelty statement to the defensible claim: one signed class policy applied consistently across admission, storage, replication, replay, recovery and ownership change, with scope-specific, re-runnable evidence. Cite sources in docs/PRIOR_ART.md. Remove any claim you cannot cite.

4. RESULTS AND REPRODUCIBILITY
   From a fresh clone on a clean machine or container, run the one-command scenario runner. Compare to the published results table: same pass/fail, medians within the stated spread. Publish docs/RESULTS.md: hardware, OS, versions, calibration of the emulator, every scenario with scope and limitations adjacent to the number, scenarios NOT RUN with reasons, baseline settings and non-normalizable differences, raw log locations. Report cases where Kamish does not help (for example single legal path). No chart without its data, run ID and caveat.

5. SECURITY AND THREAT-MODEL REVIEW
   Update the threat model (docs/THREAT_MODEL.md): assets, trust boundaries, attackers, mitigations, and residual risks, including: compromised broker with cleartext, compromised policy authority key, witness in the same failure domain, clock drift assumptions, lease TTL trade-off, label-based (not proven) location, telemetry and audit logs as regulated data, gateway session security. Run dependency and secret scans, summarize findings with severity, fix or document. List every item that would need an independent security review before any real use.

6. LIMITATIONS AND DEMO INTEGRITY
   Every README and the UI footer must carry a plain limitations section. Rewrite the demo script (docs/DEMO.md) so each step is marked REAL or SIMULATED, with the exact command and expected output. Dry-run the demo twice from a clean start and record transcripts. Prepare a "known limitations in 60 seconds" page and likely-judge Q&A (prior art, what is not built, what happens on region loss, what the audit cannot detect).

7. DOCUMENT RECONCILIATION
   Make README, project.md, worktillnow.md, ADR index, CONTRACT.md, deck text, and package READMEs mutually consistent in versions, names (Kamish vs the old Hydra Stream name; state the rename), requirement meanings, and status. Resolve contradictions explicitly in a changelog entry.

8. EXTERNAL REVIEW PACKAGE
   Produce docs/REVIEW_PACKAGE.md with: how to build, run the runner, run the probes, read the audit, and where each claim's evidence lives, so a reviewer who has never seen the project can reproduce the key results in one sitting. Include the exact commands and expected outputs.

9. READINESS DECISION
   Evaluate and report each readiness level separately with PASS/FAIL and evidence: (a) honest local demo, (b) inspectable research prototype, (c) public alpha packages, (d) anything beyond that (state plainly that production use needs independent review and is out of scope). List blockers ranked by severity and the smallest change that removes each.

ACCEPTANCE CRITERIA
- Every claim in the repo is in the register with a SUPPORTED verdict, or has been removed.
- CI banned-phrase check is green.
- Traceability matrix has no requirement with an unexplained status.
- The scenario runner reproduces published results from a fresh clone.
- Both original failing probes pass for real behavior, or the documents state clearly that they do not.
- docs/RESULTS.md has no number without a run ID and scope.
Final report: verdict per readiness level, ranked blockers, list of files changed, and an explicit statement of what was NOT verified. Do not tag or publish anything. Stop and wait for my decision.
````
