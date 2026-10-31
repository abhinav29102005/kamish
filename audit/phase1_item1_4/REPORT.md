# Phase 1, Item 1.4: Audit Witness & Probe Closure Report

## 1. Scope & Implementation
- **Cryptographic Audit Witness (W08)**: Implemented `AuditWitness` in `crates/kamish-audit` implementing independent quorum anchoring for hash-chained decision logs.
- **Signed Checkpoints**: Checkpoints contain domain-separated Ed25519 signatures (`kamish-audit-witness-checkpoint-v1`) binding `(topic, partition, sequence, tail_hash, count, timestamp_ns)` committed to consensus via `MetadataAuthority::commit_audit_checkpoint`.
- **Configurable Window & Metrics**: Supported 128-entry window cap. Added atomic telemetry tracking `witness_unreachable_count`, `time_to_anchor_ms`, and `last_anchored_count`.
- **Completeness Reporting in verify-audit**: Updated `cmd/kamish-ctl` so `verify-audit` validates the internal chain and reports completeness only up to the committed quorum anchor boundary, documenting that unanchored suffixes require witness verification (§5.2).
- **Truncation & Checkpoint Mismatch Detection**: Enforced in `AuditLog::open` and `AuditLog::open_with_witness` that if an external witness is registered, truncation below the committed checkpoint count fails closed with `AuditError::TruncationDetected`, and hash divergence fails with `AuditError::CheckpointMismatch`.
- **Probe Closure**: Updated `early_research/probes/tests/remaining_gaps.rs` with the 4 required controls from `docs/CONTRACT.md` §5.3:
  1. *Untouched positive control*: registered, checkpointed file opens and verifies successfully.
  2. *No-witness negative control*: demonstrated that without an external anchor, prefix truncation is indistinguishable from a valid log.
  3. *Anchored-prefix tamper test*: untouched original assertion `assert!(AuditLog::open(&path).is_err(), ...)` passes honestly.
  4. *Unanchored-window limitation test*: documented and verified that deletions purely within the unanchored window beyond anchor C cannot be detected by anchor C alone.

## 2. Verification Artifacts & Test Evidence
- `cargo_test_workspace.txt`: Full unedited output of `cargo test --workspace` (all 13 crates passed, 0 failures).
- `cargo_test_workspace_all_features.txt`: Full unedited output of `cargo test --workspace --all-features` (all 13 crates passed, 0 failures).
- `probes_test_output.txt`: Full unedited output of `cargo test --manifest-path early_research/probes/Cargo.toml` showing **17 passed, 0 failed** (all 11 lib probes + all 6 remaining_gaps probes are 100% green!).
- `etcd_tests_output.txt`: Full unedited output of `./scripts/run_etcd_tests.sh` showing **8 passed, 0 failed** against live etcd 3.7.3.

## 3. Phase 1 Completion Confirmation
All 4 work items of Phase 1 (Authority) are now fully implemented, verified, and audited:
- Item 1.1: `MetadataAuthority` trait and consensus decoupling.
- Item 1.2: `EtcdMetadataAuthority` on etcd 3.7.3 with mTLS, pure-Rust TLS, and 8 cluster integration tests.
- Item 1.3: Broker integration with atomic CAS partition claim, epoch fencing, multi-region fail-closed admission, zero-consensus produce path (NFR-01), and closure of probe `independent_regions_must_not_both_accept_unregistered_epoch_one_writes`.
- Item 1.4: Cryptographic audit witness (W08), signed checkpointing, 128-entry window, verify-audit completeness bounds, and closure of probe `audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable`.
