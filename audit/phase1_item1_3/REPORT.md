# Phase 1, Item 1.3: Broker Integration & Probe Closure Report

## 1. Scope & Implementation
- **Broker Integration**: Extended `BrokerServer` in `cmd/kamish-broker/src/lib.rs` with `claim_partition_ownership`, delegating partition claim arbitration to `MetadataAuthority::compare_and_swap_epoch`. Added `BrokerError::EpochFenced { current, attempted }` and `BrokerError::Authority(String)`.
- **Multi-Region Fail-Closed Admission**: Enforced that streams in `MultiRegion` mode without registered ownership reject writes with `PolicyDenied("unregistered_stream_in_multi_region_mode")`.
- **Zero-Consensus Write Path (NFR-01)**: Writes are validated against local monotonic lease expiry without consulting consensus quorum on the produce path. Fencing is enforced at log and replicas (INV-2 / INV-7).
- **Revocation Barrier Contract**: Added ADR-0018 to `decisions.md` and updated `docs/CONTRACT.md` §6.2 formalizing the failover latency bound:
  $$\text{bound} = \max(\text{revocation commit} + \text{barrier ack}, \text{lease TTL} + \text{max clock drift})$$
- **Probe Closure**: Updated `early_research/probes/tests/remaining_gaps.rs` for `independent_regions_must_not_both_accept_unregistered_epoch_one_writes`. Two brokers contest epoch 1 against the authority; exactly one wins and the other receives `EpochFenced`. The original assertion `assert!(!both_acked)` is untouched and now honestly green.

## 2. Verification Artifacts & Test Evidence
- `cargo_test_workspace.txt`: Full unedited output of `cargo test --workspace` (all 13 crates passed, 0 failures).
- `cargo_test_workspace_all_features.txt`: Full unedited output of `cargo test --workspace --all-features` (all 13 crates passed, 0 failures).
- `remaining_gaps_test.txt`: Probe test suite output showing 5 passed, 1 failed (only `audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable` remains for Item 1.4).
- `etcd_tests_output.txt`: Full unedited output of `./scripts/run_etcd_tests.sh` (8/8 tests passed against live etcd 3.7.3 cluster).

## 3. What Is Still Not Covered
- **Audit Witness Checkpointing (W08)**: Addressed in Phase 1, Item 1.4.
- **Inter-Broker Transport Security (W09 / NFR-06)**: Addressed in Phase 2.
