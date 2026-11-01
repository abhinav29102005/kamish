# Step 1 Audit Report: Mechanical Probe Compilation Alignment

**Audit Date:** 2026-11-01  
**Subsystem:** Antigravity Autonomous Security & Correctness Audit Subsystem  
**Scope:** STEP 1 — Mechanical probe fixture alignment, diff against pre-Phase-1 baseline (`77bb051`), and verification of probe pass/fail counts.  
**Artifact Directory:** `audit/phase5_remediation_step1/`

---

## 1. Scope & Execution Overview

- **Target Files:**
  - `early_research/probes/src/lib.rs`
  - `early_research/probes/tests/remaining_gaps.rs`
- **Root Cause of Breakage:**
  `ReplicationRequest` in `cmd/kamish-broker` gained 5 new fields (`context`, `source_generation`, `policy_version`, `checksum`, `envelope_payload`) during Phase 3 / Phase 5 development. Probe fixtures instantiating `ReplicationRequest` broke compilation with `error[E0063]: missing fields`.
- **Mechanical Resolution:**
  - Added the 5 missing fields to the fixture in `early_research/probes/src/lib.rs` (propagating from `range`).
  - Added the 5 missing fields to `fn transfer(...)` fixture in `early_research/probes/tests/remaining_gaps.rs`.
  - Reverted the previous invalid test tampering in `tests/remaining_gaps.rs` back to the pre-Phase-1 baseline (`77bb051`).
  - ZERO test assertions or test names were altered.

---

## 2. Test Verification Evidence

Raw test outputs preserved in `audit/phase5_remediation_step1/`:

| Probe Suite | Command | Result | Pass / Fail Details | Artifact File |
| :--- | :--- | :--- | :--- | :--- |
| **Lib Probes** | `cargo test --manifest-path early_research/probes/Cargo.toml --lib` | **PASSED** (Exit 0) | **11 passed, 0 failed** | `probes_lib_test.txt` |
| **Remaining Gaps** | `cargo test --manifest-path early_research/probes/Cargo.toml --test remaining_gaps` | **FAILED** (Exit 101) | **4 passed, 2 failed** | `probes_remaining_gaps_test.txt` |

### Detailed Test Results for `remaining_gaps`:
1. `stale_epoch_duplicate_must_not_acknowledge_a_new_epoch_history` ... **ok**
2. `duplicate_offset_with_different_content_must_be_rejected` ... **ok**
3. `claimed_multi_record_range_must_match_the_persisted_record_count` ... **ok**
4. `mixed_replication_contract_must_not_create_an_unfillable_mirror_gap` ... **ok**
5. `independent_regions_must_not_both_accept_unregistered_epoch_one_writes` ... **FAILED** (panicked at `independent regions both acknowledged partition events/0 at epoch 1`)
6. `audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable` ... **FAILED** (panicked at `removing a valid audit suffix was undetectable without an external checkpoint`)

---

## 3. Code Diff Against Baseline (`77bb051`)

```diff
diff --git a/early_research/probes/src/lib.rs b/early_research/probes/src/lib.rs
index 291ea8c..ebc5be7 100644
--- a/early_research/probes/src/lib.rs
+++ b/early_research/probes/src/lib.rs
@@ -186,6 +186,11 @@ mod review {
                 to_offset: range.end_offset,
                 payload: range.payload,
                 is_sealed: range.is_sealed,
+                context: range.context,
+                source_generation: range.source_generation,
+                policy_version: range.policy_version,
+                checksum: range.checksum,
+                envelope_payload: range.envelope_payload,
             })
             .unwrap();
         assert!(response.accepted);
@@ -336,7 +341,7 @@ mod review {
         let source = broker("a", dir.path(), DestinationMode::Cleartext);
         source
             .meta
-            .set_ownership("events", 0, "b", 7, 60_000)
+            .compare_and_swap_epoch(&kamish_meta::StreamId::new("events", 0), 0, 7, "b", 60_000)
             .unwrap();
         let result = source.produce(request(0, "class-0", b"wrong-owner".to_vec()));
         assert!(
diff --git a/early_research/probes/tests/remaining_gaps.rs b/early_research/probes/tests/remaining_gaps.rs
index d90a927..362b5bf 100644
--- a/early_research/probes/tests/remaining_gaps.rs
+++ b/early_research/probes/tests/remaining_gaps.rs
@@ -72,6 +72,11 @@ fn transfer(epoch: u64, to_offset: u64, payload: &[u8]) -> ReplicationRequest {
         to_offset,
         payload: payload.to_vec(),
         is_sealed: false,
+        context: None,
+        source_generation: 1,
+        policy_version: 1,
+        checksum: None,
+        envelope_payload: None,
     }
 }
```

---

*Report compiled for STEP 1. Uncommitted changes retained in working tree. Awaiting user go-ahead.*
