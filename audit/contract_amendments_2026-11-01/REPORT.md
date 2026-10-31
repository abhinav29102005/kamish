# Contract amendments — 1 November 2026

## 1. Item and changed files

Applied the maintainer's Phase 0 approval amendments to `docs/CONTRACT.md` and
ADR-0015 in `decisions.md`; updated `worktillnow.md`. The full contract/ADR diff
was displayed before consensus-candidate research and is retained in
[contract-amendments.diff](contract-amendments.diff), against `9955f77`.

- Option 1 (three in-region payload replicas; two durable before ACK) follows
  Phase 1. Synchronous K-region durability remains optional later work.
- Ownership uses time-limited quorum-issued leases with local per-write checks.
  The old per-write metadata completion proposal is explicitly superseded.
- Quorum-issued source identity/generation is required for watermarks.
- Revocation must cancel in-flight work and block further use/egress. The global
  effective-time boundary under disconnected leases needs clarification.
- Witness independence, threat-model limits, and a proposed 128-entry unanchored
  cap are documented. Its numeric value awaits confirmation; a 1-second scheduler
  target is not a time-to-anchor guarantee.
- Audit probe/fixture changes are reserved for the witness implementation commit,
  preserving the existing tamper assertion and adding all requested controls.

## 2. Checks and results

No runtime tests were added or modified. This item changes documentation only.
Exact commands and exit codes are recorded in [checks.json](checks.json):

```bash
git diff --check -- '*.md'
git diff --exit-code 9955f77 -- cmd crates packages frontend early_research/probes scripts proto Cargo.toml Cargo.lock result.txt
```

Both commands exit 0 with empty stdout/stderr. The second checks all listed
runtime/test/manifests against the prior documentation commit. A byte comparison
of `early_research/probes/tests/remaining_gaps.rs` against that revision also
passes, including the original audit tamper assertion.

The last runtime baseline remains [the Phase 0 run](../phase0_baseline_2026-10-31_204424Z/REPORT.md):
102 workspace tests, 11 original probes, 4 frontend tests, 23 SDK tests and one
integration test passed; extended probes had four passes and two failures. Those
numbers are historical baseline results, not a new test execution in this item.

## 3. Still not covered

Lease protocol, storage/consensus adapters, audit witness, revocation transport
gates and payload quorum have no new implementation. The three requested audit
controls and untouched-file positive control are requirements for W08, not tests
that exist or passed as a result of this documentation amendment.

The numeric audit cap and lease parameters are not silently selected. The
revocation-barrier option distinguishes request/publication from confirmed global
effectiveness; this needs a maintainer answer rather than an implementation guess.

## 4. Requirement status

- FR-07/08 and INV-7: authority/fencing still open; independent-owner probe retains
  its baseline failure.
- FR-13: anchored completeness still open; audit-tail probe retains its baseline
  failure.
- NFR-01: approved target is lease-based, off the per-write metadata path; still
  open until implemented/tested. No indefinite continuation beyond expiry claimed.
- INV-2/3: approved payload quorum/recovery work follows Phase 1; still open.
- FR-04/05/06, NFR-02, INV-5/6/8/9: strengthened identity/revocation requirements
  documented; existing scoped implementations unchanged.

Next: present consensus license/maintenance/fit evidence and wait for the
maintainer's pick before adding a consensus dependency or implementing W01.
