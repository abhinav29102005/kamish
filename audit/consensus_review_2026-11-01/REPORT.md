# Phase 1 preparation — consensus review, 1 November 2026

## 1. Item and files

Reviewed candidates after applying/showing the Phase 0 amendments in `c516321`.
Added [docs/CONSENSUS_CANDIDATES.md](../../docs/CONSENSUS_CANDIDATES.md), ADR-0016
in `decisions.md`, and the current handover entry in `worktillnow.md`.

The comparison recommends OpenRaft 0.9.25 for its Rust/Tokio fit, compares external
etcd 3.7.3 and raft-rs 0.7.0, and records license, release/maintenance observations,
source links and adoption limits. The maintainer's selection is pending.

## 2. Tests and checks

No runtime tests added, changed or rerun for this documentation/source-review
item. No candidate was installed or executed. Latest runtime test evidence remains
the [Phase 0 baseline](../phase0_baseline_2026-10-31_204424Z/REPORT.md).

Documentation validation commands/results are recorded in [checks.json](checks.json).
The Git checks are:

```bash
git diff --check -- '*.md'
git diff --exit-code 9955f77 -- cmd crates packages frontend early_research/probes scripts proto Cargo.toml Cargo.lock result.txt
```

Both exit 0 with empty output. Markdown local-link/fence/anchor/whitespace checks
also pass for the amended contract, ADRs, handover, comparison and new reports.
Byte comparison confirms the entire extended probe file remains identical to
`9955f77`, including the audit tamper assertion.

Source review is not a runtime test. Release dates, licenses and API behavior
are supported by the source register in the comparison. Failed HTTP requests and
the distinction between registry and documentation dates are recorded there.

## 3. Still not covered

- No selected dependency or artifact installed; no full transitive-license or
  security advisory audit, build compatibility test or storage-adapter evaluation.
- No implemented consensus service, grants, fences, witness, or revocation barrier.
- No evidence of independent physical failure domains on this emulation host.
- Numeric lease parameters, the proposed 128-entry audit cap/1-second scheduling
  target and global revocation effectiveness still need explicit decisions.
- None of the four newly required audit controls has been added; they belong in
  W08 with the fixture diff and unchanged tamper assertion.

## 4. Requirement status and stop gate

FR-07/08, INV-7 and W01/W02/W05 remain open; FR-13/W08 remain open. NFR-01 is the
approved lease-based target but has no new implementation evidence. INV-2/INV-3
payload quorum/recovery remains scheduled after Phase 1. No W item is closed.

**Stop for maintainer pick:** OpenRaft, external etcd, or raft-rs. No implementation
begins from the recommendation alone.
