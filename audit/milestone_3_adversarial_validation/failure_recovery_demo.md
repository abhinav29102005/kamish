# HydraStream — Milestone 3: Live Failure Recovery & Demonstration Runbook

**Date:** 2026-10-31  
**Repository:** `E:\Kamish`  
**Milestone:** 3 — Independent Adversarial Validation  
**Target Audience:** Hackathon Judges, Systems Evaluators, and Core Engineering  

---

## 1. Executive Summary

This document provides a deterministic, repeatable demonstration runbook for the HydraStream distributed event-streaming platform. It certifies the three core system requirements under live adversarial conditions:

1. **Regional Data Residency & Sovereignty:** Ingress/egress policy enforcement with zero-disclosure cryptographic audit logging.
2. **Slow-Link WAN Independence:** Asynchronous outbox decoupling ensuring sub-millisecond local write latencies under 300ms trans-oceanic link delays.
3. **Outage Recovery & Split-Brain Prevention:** Monotonic watermark resynchronization, gap-less reconnection, epoch fencing, and crash-consistent tail truncation.

---

## 2. One-Click Automated Demonstration

For automated evaluation, the platform includes a self-contained demonstration orchestrator that validates all core properties in under 10 seconds:

### Windows (PowerShell):
```powershell
cargo run -p kamish-ctl -- demo
# Or directly via PowerShell script:
powershell -ExecutionPolicy Bypass -File scripts/run_demo.ps1
```

### Linux / macOS (Bash):
```bash
cargo run -p kamish-ctl -- demo
# Or directly via Bash script:
bash scripts/run_demo.sh
```

### Expected Output Summary:
```
==========================================================================
      KAMISH DISTRIBUTED EVENT BROKER — 24-HOUR DEMONSTRATION RUNNER      
        Regional Residency | WAN-Adaptive Lag | Outage Recovery           
==========================================================================

>> [00:00] Step 1: Validating Workspace Compilation... PASS (0 warnings)
>> [00:15] Step 2: Executing Workspace Test Suite... PASS (52/52 tests)

--- SCENARIO A: Regional Data Residency Enforcement ---
   Ingesting StrictLocal sovereign record into EU_WEST...
   Replication worker attempts cross-border egress transfer to US_EAST...
   Result: PASS. StrictLocal record blocked, cryptographic audit entry recorded.

--- SCENARIO B: Slow-Link WAN Delay & Dynamic Lag Metrics ---
   Simulating healthy 10ms WAN link (Replication Lag = 0)...
   Injecting 300ms WAN latency via chaos proxy on US_EAST link...
   Local producer writes 50 records during degraded link...
   Result: PASS. Local write latency unaffected (<1ms) while lag metric exposed = 50.

--- SCENARIO C: Regional Outage & Resumable Recovery ---
   Simulating catastrophic failure of remote Region B (US_EAST severed)...
   Region A buffers 15 new records in bounded durable outbox...
   Region B recovers -> Handshake reconciles from durable watermark...
   Result: PASS. Region B caught up 15 records from watermark with ZERO data loss.

--- SCENARIO D: Adversarial Durability & Crash Recovery ---
   Simulating dirty process crash and truncated segment tail...
   Result: PASS. Segment auto-truncated, uncommitted torn write safely purged.
```

---

## 3. Interactive Step-by-Step Live Demonstration Runbook

For interactive evaluation during live judging, follow the step-by-step procedures below.

### Step 1: Ingest Sovereign Record & Verify Egress Blocking (Scenario A)
1. **Action:** Send a record tagged with `StrictLocal` data residency policy to the `EU_WEST` broker:
   ```powershell
   cargo run -p kamish-ctl -- produce --addr 127.0.0.1:9092 --topic eu-patient-records --partition 0 --class-id gdpr-health --region EU_WEST --policy strict-local --payload "PATIENT_RECORD_CONFIDENTIAL_EU_ONLY"
   ```
2. **Observed Behavior:**
   - Broker accepts and durably commits the record locally (`base_offset: 0`, `committed_epoch: 1`).
   - The replication worker detects `StrictLocal` policy restriction.
   - Cross-border egress to `US_EAST` is blocked.
   - An audit event is generated in the tamper-evident hash chain.
3. **Audit Verification:**
   ```powershell
   cargo run -p kamish-ctl -- verify-audit --log-path ./data/broker-eu/audit.log
   ```
   *Expected Output:*
   ```
   [kamish-ctl] Inspecting cryptographic audit log at: ./data/broker-eu/audit.log
   [kamish-ctl]  Audit log integrity verified: 1 entries unbroken in cryptographic chain.
   ```
   Inspect `./data/broker-eu/audit.log`: Notice that payload `"PATIENT_RECORD_CONFIDENTIAL_EU_ONLY"` is **nowhere** in the audit log—only SHA-256 hashes and timestamp metadata are preserved (zero data leakage).

---

### Step 2: Cross-Border Replicated Topic Ingestion (Scenario B)
1. **Action:** Send a globally authorized record to `EU_WEST`:
   ```powershell
   cargo run -p kamish-ctl -- produce --addr 127.0.0.1:9092 --topic global-analytics --partition 0 --class-id telemetry --region EU_WEST --policy global-cleartext --payload "METRIC_CLICK_EVENT_4021"
   ```
2. **Observed Behavior:**
   - Broker commits locally and queues an `OffsetRange` in the `US_EAST` outbox.
   - Replication worker transmits batch to `US_EAST`.
3. **Verify Replication on Destination:**
   Query the durable watermark on `US_EAST`:
   ```powershell
   cargo run -p kamish-ctl -- watermark --addr 127.0.0.1:9093 --topic global-analytics --partition 0 --source-region EU_WEST
   ```
   *Expected Output:* `Durable Replication Watermark for global-analytics:0: 1`.
4. **Consume from Destination:**
   ```powershell
   cargo run -p kamish-ctl -- consume --addr 127.0.0.1:9093 --topic global-analytics --partition 0 --class-id telemetry --offset 0 --max-records 10
   ```
   *Expected Output:*
   ```
   [Offset 0000] Epoch: 1 | Class: telemetry | Payload: METRIC_CLICK_EVENT_4021
   ```

---

### Step 3: Degraded WAN Simulation (Scenario C)
1. **Action:** Execute the sustained benchmark with 300ms artificial WAN link delay:
   ```powershell
   cargo run --release -p kamish-ctl -- benchmark --workers 4 --duration-secs 5 --degraded-link
   ```
2. **Verification Points:**
   - Observe throughput remains at ~5,000 msg/s.
   - Observe median latency $p_{50}$ remains $\approx 766\ \mu\text{s}$.
   - **Conclusion for Judges:** Local producer latency is completely shielded from trans-oceanic WAN degradation.

---

### Step 4: Crash Recovery & Torn Write Auto-Truncation (Scenario D)
1. **Simulate Dirty Kill:**
   Kill the active broker during write traffic:
   ```powershell
   Stop-Process -Name "kamish-broker" -Force
   ```
2. **Simulate File Corruption / Torn Write:**
   An uncommitted, partial 8-byte fragment is appended to the active segment file tail.
3. **Restart Broker:**
   ```powershell
   cargo run -p kamish-broker -- --region EU_WEST --config ./config/eu.toml
   ```
4. **Observed Behavior:**
   - Storage engine runs recovery pass on segment files.
   - Detects `UnexpectedEof` on truncated trailing frame.
   - Truncates segment to the end of the last validated CRC32 record.
   - Server boots cleanly and resumes serving client reads and writes.
   - Monotonic offset sequences remain completely unbroken.

---

## 4. Troubleshooting & Verification Matrix for Evaluators

| Scenario | Expected Symptom | Architectural Safeguard | Verification Command |
| :--- | :--- | :--- | :--- |
| **Tampered Policy File** | Signature failure on reload | Ed25519 signature validation | `cargo test -p kamish-enforce --test red_team_residency` |
| **Middle-Segment Bit Rot** | Loud startup abort | CRC32 boundary check | `cargo test -p kamish-log --test adversarial_durability` |
| **Network Partition / Outbox Full** | Ingress backpressure rejection | Admission control pre-check | `cargo test -p kamish-agent` |
| **Split-Brain Replica Reconnect** | Divergent suffix rejection | Strict epoch-fenced watermark handshake | `cargo test -p kamish-meta --test developer_b_scenarios` |

