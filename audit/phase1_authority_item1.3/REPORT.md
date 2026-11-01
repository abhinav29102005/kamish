# Phase 1 — Item 1.3 Audit Report: Two-Broker Ownership & Multi-Region CAS

## 1. Item and Files Changed
- **Work Item**: Prompt A, Part 1 — Item 1.3: Two-broker ownership test (W01/W02/W05)
- **Files Changed**:
  - `early_research/probes/tests/remaining_gaps.rs`: Parameterized `broker_with_authority` allowing multi-region authority injection into probe harnesses; updated `independent_regions_must_not_both_accept_unregistered_epoch_one_writes` fixture to use `DeploymentMode::MultiRegion`.
  - `crates/kamish-meta/src/lib.rs`: Restricts `set_ownership` so `MultiRegion` mode fails closed and cannot bypass `compare_and_swap_epoch`.

## 2. Test Execution & Evidence
- **Exact Command**:
  `cargo test --manifest-path early_research/probes/Cargo.toml --test remaining_gaps`
- **Result**:
  - `test independent_regions_must_not_both_accept_unregistered_epoch_one_writes ... ok` (PASS)
  - Raw output saved to `audit/phase1_authority_item1.3/raw_test_output.txt`.

## 3. What is Still Not Covered
- `audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable`: Fails as expected until external audit witness and signed checkpoint commits are completed in Item 1.4.
- Physical multi-process etcd cluster execution remains gated under `KAMISH_ETCD_TESTS=1`.

## 4. Requirement Status
- **W01 / W02 / W05 (Multi-Region Partition Ownership & Fencing)**: Implemented & Tested in probe fixture; unregistered partitions in multi-region mode fail closed before write admission.
