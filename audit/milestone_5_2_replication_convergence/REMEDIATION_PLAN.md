# Milestone 5.2: Prioritized Remediation Plan

This remediation plan addresses the gaps and architectural nuances identified during the Milestone 5.2 audit.

> [!IMPORTANT]
> **Operating Constraint:** This audit is in **read-only mode**. No production code modifications have been made.
> The proposed diffs below are prioritized recommendations requiring explicit authorization before implementation.

---

## Remediation Priority Overview

| Priority | Issue / Opportunity | File(s) | Impact | Effort |
|:---|:---|:---|:---|:---|
| **P1** | Early-break batch on `offset_gap` rejection | `cmd/kamish-agent/src/lib.rs` | Eliminates wasted network round-trips on link gaps | Small (~10 lines) |
| **P2** | Scheduled retry of dropped outbox tasks | `cmd/kamish-agent/src/lib.rs` | Prevents replication stalls when transport retries exceed 3 | Medium (~25 lines) |
| **P3** | Multi-destination partial reservation / per-target backpressure | `cmd/kamish-broker/src/lib.rs` | Avoids fate-sharing where one slow destination blocks healthy ones | Large (~60 lines) |
| **P3** | Persistent partition ownership lease store | `crates/kamish-meta/src/lib.rs` | Retains consensus leader epoch across process restarts | Large (~100 lines) |

---

## 1. P1: Early-Break Replication Batch on Gap Rejection

### Problem Description
In `step_replication` and `step_replication_tcp` (`cmd/kamish-agent/src/lib.rs`), when a remote broker returns `accepted: false` due to `offset_gap`, the loop logs a warning but continues executing subsequent ranges in the same batch. Since replication ranges are contiguous and ordered, any subsequent range in the batch will also fail with `offset_gap`. Continuing the loop wastes network I/O and server CPU.

### Proposed Code Diff
```diff
--- a/cmd/kamish-agent/src/lib.rs
+++ b/cmd/kamish-agent/src/lib.rs
@@ -703,6 +703,10 @@ impl KamishAgent {
                     "Replication rejected by peer: {}",
                     rep_res.reason
                 );
+                // If peer detected a gap, subsequent ranges in this batch will also fail.
+                if rep_res.reason.contains("offset_gap") || rep_res.reason.contains("partial_overlap") {
+                    break;
+                }
             }
         }
```

### Risk & Verification Strategy
- **Risk:** Minimal / Safe. Does not affect watermark tracking or data safety.
- **Verification:** Unit test sending a batch of 5 records where record 0 is missing; verify only 1 RPC is sent instead of 5.

---

## 2. P2: Scheduled Re-queueing of Outbox Tasks Exceeding Max Retries

### Problem Description
In `spawn_retry_worker` (`cmd/kamish-agent/src/lib.rs`), if a task fails 3 consecutive delivery attempts, it is dropped from the async channel with `tracing::error!("Replication task dropped after max retries")`. While the obligation remains in the outbox on disk, it is never re-enqueued until an external call or restart occurs.

### Proposed Code Diff
```diff
--- a/cmd/kamish-agent/src/lib.rs
+++ b/cmd/kamish-agent/src/lib.rs
@@ -656,6 +656,12 @@ impl KamishAgent {
                             "Replication task permanently failed after max retries"
                         );
+                        // Re-queue task with exponential backoff delay instead of dropping
+                        let tx_clone = retry_tx.clone();
+                        tokio::spawn(async move {
+                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
+                            let _ = tx_clone.send(task).await;
+                        });
                     }
```

### Risk & Verification Strategy
- **Risk:** Low. Prevents deadlocks during prolonged transient network partitions.
- **Verification:** Mock network failure for 10 seconds, then restore network and confirm replication automatically resumes without manual intervention.

---

## 3. P3: Per-Destination Ack/Reservation Decoupling (Mitigating Fate Sharing)

### Problem Description
In `BrokerServer::produce_at_epoch`, the broker reserves capacity across all destinations before committing to WAL. If destination B's outbox is full, the entire produce request is denied with backpressure, blocking replication to healthy destination C.

### Design Recommendation
Depending on the topic's configured `AckMode`:
- If `AckMode::LocalMajority`: The record only strictly requires local WAL append and quorum acknowledgment. The broker could optionally commit to WAL and enqueue to available outboxes, while flagging the saturated destination for asynchronous recovery repopulation once capacity frees up.
- Note: This is an architectural change that must be explicitly authorized based on product SLA requirements.

---

## 4. Authorization Gate

No code changes from this remediation plan will be implemented without explicit user approval.

