# Phase 0 baseline and contract proposal — 1 November 2026

**Baseline matches the requested starting point. Phase 0 is a proposal awaiting
approval; no implementation work item is closed.**

## 1. Item and files

- Read `worktillnow.md`, `project.md`, the closure/source reviews and the latest
  resolution report in the requested order before making edits.
- Recorded this new evidence directory before editing project documentation.
- Baseline source revision: `77bb051bfa77e6be80e8d8e5f4ea3cd4dad9eba2` (frontend
  logo integration after the handover's original `170de8e` snapshot).
- Added proposed [docs/CONTRACT.md](../../docs/CONTRACT.md) and approval options in
  [decisions.md](../../decisions.md#adr-0015-phase-0-contract-and-approval-gates).
- Updated the handover, project novelty/demo wording and active early-research
  summaries. Historical evidence and the two failing probes are preserved.
- `worktillnow.md` and `Screenshot_20261101_020231.png` were already untracked at
  baseline. The handover is part of this documentation work; the screenshot is
  not a test artifact and is excluded from it.

The directory uses the machine's UTC start timestamp, **31 October 2026
20:44:24Z**, corresponding to **1 November 2026 02:14:24 IST**. Date labels are
not altered to imply a different execution time.

## 2. Tests, exact commands and results

No tests were added or changed in this documentation-only phase. The baseline
commands ran once, before the contract edits. Toolchain: Rust/Cargo 1.99.1,
Node v22.22.0, npm 10.9.4; full environment, durations, exit codes and output
SHA-256 values are in [checks.json](checks.json).

| Working directory | Exact command | Result / raw output |
| --- | --- | --- |
| Repository root | `cargo fmt --all -- --check` | Exit 0; [empty successful output](results/rust-format.txt) |
| Repository root | `cargo clippy --workspace --all-targets -- -D warnings` | Exit 0; [output](results/rust-clippy.txt) |
| Repository root | `cargo test --workspace` | Exit 0; 102 passed, none failed/ignored/filtered; [complete output](results/rust-tests.txt) |
| Repository root | `cargo test --locked --manifest-path early_research/probes/Cargo.toml --target-dir target -- --test-threads=1` | Exit 101; original 11 pass, extended 4 pass / 2 fail; [complete output](results/research-probes.txt) |
| `frontend/` | `npm run typecheck` | Exit 0; [output](results/frontend-typecheck.txt) |
| `frontend/` | `npm run lint` | Exit 0; [output](results/frontend-lint.txt) |
| `frontend/` | `npm test` | Exit 0; 4 pass; [output](results/frontend-tests.txt) |
| `frontend/` | `npm run build` | Exit 0 with graphics chunk warning; [output](results/frontend-build.txt) |
| `packages/kamish-client/` | `npm run typecheck` | Exit 0; [output](results/sdk-typecheck.txt) |
| `packages/kamish-client/` | `npm test` | Exit 0; 23 pass; [output](results/sdk-tests.txt) |
| `packages/kamish-client/` | `npm run test:integration` | Exit 0; 1 real Rust/Node restart integration pass; [output](results/sdk-rust-integration.txt) |
| `packages/kamish-client/` | `npm run test:package` | Exit 0; isolated package consumer imports/declarations pass; [output](results/sdk-package-consumer.txt) |

`baseline_matches: true` means the observed baseline matches the requested
starting state. **It does not turn the exit-101 probe suite into a passing suite.**
The 102 count is the sum of package test-result lines, not an invented single
Cargo summary line. The raw files contain combined stdout/stderr bytes without
rewritten test messages. Hashes establish file correspondence, not external trust.

### Unedited research-probe output

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.09s
     Running unittests src/lib.rs (target/debug/deps/kamish_repo_review-74cc536a8ac93ab4)

running 11 tests
test review::ack_of_one_partition_preserves_other_partition ... ok
test review::audit_hash_binds_field_boundaries ... ok
test review::broker_obeys_metadata_owner ... ok
test review::concurrent_retries_append_once ... ok
test review::fetch_cannot_bypass_record_policy_with_another_class ... ok
test review::oversized_outbox_write_is_not_acknowledged ... ok
test review::pending_replication_survives_broker_restart ... ok
test review::recovery_preserves_per_record_residency ... ok
test review::retry_after_restart_is_idempotent ... ok
test review::sealed_destination_does_not_persist_plaintext ... ok
test review::signed_policy_survives_wire_roundtrip ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

     Running tests/remaining_gaps.rs (target/debug/deps/remaining_gaps-7af4a35fd804135a)

running 6 tests
test audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable ... FAILED
test claimed_multi_record_range_must_match_the_persisted_record_count ... ok
test duplicate_offset_with_different_content_must_be_rejected ... ok
test independent_regions_must_not_both_accept_unregistered_epoch_one_writes ... FAILED
test mixed_replication_contract_must_not_create_an_unfillable_mirror_gap ... ok
test stale_epoch_duplicate_must_not_acknowledge_a_new_epoch_history ... ok

failures:

---- audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable stdout ----

thread 'audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable' (656229) panicked at tests/remaining_gaps.rs:205:5:
removing a valid audit suffix was undetectable without an external checkpoint
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- independent_regions_must_not_both_accept_unregistered_epoch_one_writes stdout ----

thread 'independent_regions_must_not_both_accept_unregistered_epoch_one_writes' (656232) panicked at tests/remaining_gaps.rs:147:5:
independent regions both acknowledged partition events/0 at epoch 1


failures:
    audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable
    independent_regions_must_not_both_accept_unregistered_epoch_one_writes

test result: FAILED. 4 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s

error: test failed, to rerun pass `--test remaining_gaps`
```

## 3. What is still not covered

- No authority, audit witness, quorum payload storage or secure transport was
  implemented. Both extended failures remain.
- Dependency advisory scans, browser automation, Lighthouse, CLI smoke and
  separate browser-crypto probes were not rerun here; their older evidence is
  not relabeled as fresh validation.
- Graphics chunk remains 635.62 kB, with the recorded Vite warning. Main entry
  is 247.50 kB. These are build outputs, not device performance measurements.
- Baseline uses the installed dependencies and local untracked root Cargo.lock;
  lockfile hashes are recorded, but clean-room dependency reproduction is not
  established by this run.
- Contract decisions need approval, particularly strict authority versus NFR-01,
  K-region semantics, checkpoint coverage, audit fixture setup and disposition.

## 4. Affected requirement status

| IDs | Status after this item |
| --- | --- |
| FR-07/08, INV-7 | Local guard only; independent-owner contract fails |
| FR-13 | Internal audit integrity implemented locally; anchored completeness fails |
| INV-2/3, FR-02/03 | Proposed quorum/recovery contract; local file-sync tests do not implement it |
| FR-05/06, INV-1/5/6 | Existing scoped tests; source-generation/full-envelope/durable-cursor work pending |
| FR-04, NFR-02, INV-8/9 | Existing local policy checks; durable floor and revocation/disposition work pending |
| NFR-01 | Unimplemented; proposed strict authority option explicitly leaves it pending |
| NFR-05 | This baseline has revision, commands, toolchain and raw outputs; broader experiments pending |
| NFR-06 | Unimplemented; plaintext transport remains |

**Next action:** maintainer review of [the contract](../../docs/CONTRACT.md) and
ADR-0015. Phase 1 starts only after approval and resolution of blocking choices.
