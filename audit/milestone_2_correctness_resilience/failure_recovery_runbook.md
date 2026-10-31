# Milestone 2 — Failure Recovery & Hackathon Demonstration Runbook

**Project:** HydraStream / Kamish Distributed Event Streaming Engine  
**Purpose:** Step-by-step operational runbook for executing live hackathon demonstrations of failure injection, residency enforcement, and automated recovery.  
**Target Audience:** Hackathon judges, systems architects, reliability engineers.  

---

## 1. Overview of Demonstration Scenarios

This runbook provides reproducible operational procedures for four high-impact distributed systems failure scenarios:
1. **Scenario 1: Sudden Broker Crash & Torn Write Tail Recovery**
2. **Scenario 2: Slow Link Degradation, Outbox Backpressure & Drain**
3. **Scenario 3: Network Partition, Epoch Fencing & Suffix Quarantine**
4. **Scenario 4: Regional Data Residency Breach Prevention & Canary Detection**

All scenarios can be demonstrated directly using `kamish-ctl` or verified through in-tree automated test suites.

---

## 2. Scenario 1: Sudden Broker Crash & Torn Write Tail Recovery

### Objective
Demonstrate that when a broker process crashes mid-write (e.g., kill -9 or sudden power loss) leaving partial/torn bytes at the segment tail, the broker restarts cleanly, automatically truncates corrupt trailing bytes back to the last valid CRC32 envelope, and continues serving writes without data loss or offset gaps.

### Step-by-Step Execution
1. **Start Broker with Persistent Storage:**
   ```bash
   cargo run -p kamish-broker -- --port 9092 --data-dir ./data/demo-p0 --region eu-west-1
   ```
2. **Produce 100 Valid Messages:**
   ```bash
   cargo run -p kamish-ctl -- produce -p 9092 -t telemetry --partition 0 -m "sensor-event" -c 100
   ```
3. **Simulate Dirty Kill & Torn Write:**
   Terminate the broker process immediately (`Ctrl+C` or taskkill):
   ```powershell
   # Append incomplete garbage bytes directly to the end of partition.data
   $data = [byte[]](0x00, 0x00, 0x05, 0xFF, 0xDE, 0xAD, 0xBE, 0xEF)
   [System.IO.File]::AppendAllBytes("./data/demo-p0/partition.data", $data)
   ```
4. **Restart Broker:**
   ```bash
   cargo run -p kamish-broker -- --port 9092 --data-dir ./data/demo-p0 --region eu-west-1
   ```
5. **Verify Tail Truncation in Broker Logs:**
   The broker logs:
   ```text
   WARN Detected torn trailing write in log. Truncating segment to last verified CRC32 boundary (offset: 99).
   INFO PartitionLog opened successfully: high_watermark=100 next_offset=100
   ```
6. **Produce Next Message (Offset 100):**
   ```bash
   cargo run -p kamish-ctl -- produce -p 9092 -t telemetry --partition 0 -m "recovered-event" -c 1
   ```
   *Expected Outcome:* Base offset returned is strictly `100`, confirming unbroken monotonic continuity.

---

## 3. Scenario 2: Slow Link Degradation, Outbox Backpressure & Drain

### Objective
Demonstrate that when an inter-region link degrades (high latency / packet drops), local producer ingestion remains unblocked while the outbox buffers records, respects bounded memory limits, and rapidly drains once the link heals.

### Step-by-Step Execution
1. **Inject Chaos Delay on Inter-Region Route:**
   ```bash
   # Injects 50ms synthetic latency and 3% jitter to us-east-1 route
   cargo run -p kamish-ctl -- chaos inject --target us-east-1 --delay-ms 50 --jitter-pct 3
   ```
2. **Produce Continuous Stream into Local Broker (`eu-west-1`):**
   ```bash
   cargo run -p kamish-ctl -- benchmark -p 9092 -c 2000 -j 4
   ```
3. **Observe Local vs Replication Performance:**
   - Local producer latency remains low ($p_{50} < 1\text{ ms}$).
   - The outbox backlog increases linearly as records wait for the slow link.
4. **Inspect Replication Lag:**
   ```bash
   cargo run -p kamish-ctl -- status --region eu-west-1
   ```
   *Expected Output:*
   ```text
   Topic: telemetry | Partition: 0
   Local High-Water Mark: 2000
   Remote Watermark (us-east-1): 450
   Replication Lag: 1550 messages
   Outbox Utilization: 396.8 KB / 50.0 MB (Bounded)
   ```
5. **Heal Link and Observe Drain:**
   ```bash
   cargo run -p kamish-ctl -- chaos reset --target us-east-1
   ```
   Within seconds, the replication worker drains the outbox at peak wire speed until Replication Lag reaches `0`.

---

## 4. Scenario 3: Network Partition, Epoch Fencing & Suffix Quarantine

### Objective
Demonstrate that an isolated old leader cannot corrupt partition state when a newer epoch leader has been elected, and any divergent local writes are quarantined rather than silently discarded or overwritten.

### Step-by-Step Execution
1. **Promote New Leader with Incremented Epoch:**
   ```bash
   cargo run -p kamish-ctl -- leader promote -t telemetry -p 0 --region us-east-1 --epoch 2
   ```
2. **Attempt Write by Stale Leader (`epoch 1`):**
   A client submits a write with `epoch: 1` to the old leader broker.
   *Expected Output:*
   ```text
   ERROR Broker returned StaleEpoch: current partition epoch is 2, requested 1
   ```
3. **Reconcile Divergent Suffix on Reconnect:**
   When the partition heals and the old broker syncs from the new leader:
   ```bash
   cargo run -p kamish-ctl -- reconcile -t telemetry -p 0
   ```
   *Expected Log:*
   ```text
   INFO Divergent suffix detected: 3 uncommitted local records.
   INFO Quarantining divergent records [offset 101..103] into ./quarantine/telemetry-0-epoch1/
   INFO Resetting local watermark to match authoritative leader watermark (101).
   ```

---

## 5. Scenario 4: Regional Data Residency Breach Prevention

### Objective
Demonstrate cryptographically enforceable regional data residency boundaries: `StrictLocal` data is prevented from leaving the sovereign region, and unauthorized cross-border fetch attempts fail with audit trail evidence.

### Step-by-Step Execution
1. **Ingest Sovereign Record with StrictLocal Residency:**
   ```bash
   cargo run -p kamish-ctl -- produce -p 9092 -t sovereign-data --residency StrictLocal -m "EU-CITIZEN-RECORD-001"
   ```
2. **Attempt Cross-Region Replication to `us-east-1`:**
   Observe agent log output:
   ```text
   WARN Egress replication interceptor hook BLOCKED transfer: event residency StrictLocal prohibits replication to us-east-1
   INFO Recorded AuditEntry #42: action=ReplicationEgress, outcome=DENIED, hash=e3b0c44...
   ```
3. **Verify Audit Trail Integrity:**
   ```bash
   cargo run -p kamish-ctl -- audit verify --topic sovereign-data
   ```
   *Expected Output:*
   ```text
   Audit Chain Verification:
   Entries: 42
   Chain Integrity: VALID
   Tamper Detected: NONE
   Last Hash: b5a78c1...
   ```

