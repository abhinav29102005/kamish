# Phase 4 Scenario Run Evidence: `run_20261101_012512`

- **Timestamp**: 2026-11-01T01:25:51.612588700+00:00
- **Git Commit**: `6d507b4147b702592ce87fffe0e16e7e40ca7c4f`
- **Scenario**: `three_region_evidence`
- **Seed**: `42`
- **Verdict**: **ALL_INVARIANTS_PASSED**

## 1. Network Transport Impairment & Delay Calibration (W31)

| Link | Configured Latency (1-way) | Achieved p50 (Median) | Achieved p90 | Achieved p95 | Achieved p99 | IQR Spread | StdDev | Error % |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `EU_WEST -> US_EAST` | 70.0 ms | 70.35 ms | 72.33 ms | 72.80 ms | 72.81 ms | 3.92 ms | 2.74 ms | 0.50% |
| `EU_WEST -> AP_SOUTH` | 130.0 ms | 130.00 ms | 134.27 ms | 134.50 ms | 135.33 ms | 7.96 ms | 5.50 ms | 0.00% |
| `US_EAST -> AP_SOUTH` | 190.0 ms | 190.42 ms | 196.04 ms | 196.52 ms | 197.53 ms | 12.23 ms | 8.37 ms | 0.22% |

*Raw latency samples for each link are recorded in JSON artifacts alongside this report.*

## 2. Invariant & Durability Verification (W30, INV-1 through INV-7)

| Invariant ID | Description | Status | Evidence Details |
| :--- | :--- | :--- | :--- |
| `W31-CALIBRATION` | Transport Delay Calibration across 3 WAN Links | **PASS** | 3 links calibrated with seed 42; max error was 0.50% |
| `INV-2/INV-3-QUORUM` | In-region Quorum 2-of-3 Persist Durability | **PASS** | Write committed and acknowledged after 2 of 3 physical persists |
| `W30-HISTORY-CHECKER` | History-based Loss, Duplicate, Ordering, and Epoch Checker | **PASS** | 0 data loss, 0 duplicates, 0 ordering gaps across 200 verified records |
| `W30-CANARY-SCANNER` | Storage & Wire Canary Token Scanner with Positive Control | **PASS** | Clean storage verified clean; positive control caught deliberate leak in decoded storage |

## 3. History Checker & Canary Positive Control Summary

- **Total Producer Acknowledged Records**: 200
- **Total Consumer Verified Records**: 200
- **Data Loss Violations**: 0
- **Duplicate Record Violations**: 0
- **Ordering Gaps**: 0
- **Epoch Conflicts**: 0
- **Canary Positive Control Caught Leak**: **YES (PASS)**

