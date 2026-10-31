# Milestone 5.2: Failure Matrix & Boundary Analysis

This document provides a systematic breakdown of failure windows and fault injection scenarios evaluated during the Milestone 5.2 audit.

---

## 1. Outbox Capacity & Admission Failures (Audit A)

| ID | Fault Injected | Code Location | System Response | Invariants Protected | Residual Risk |
|:---|:---|:---|:---|:---|:---|
| **A1** | Producer arrives when destination outbox is saturated | `cmd/kamish-broker/src/lib.rs:525` | `reserve_capacity` fails with `QueueCapacityExceeded`. Pre-allocated reservations for prior peers are dropped via RAII. Backpressure returned to producer without touching WAL. | R1, R2 | **None** |
| **A2** | Record size exceeds reservation estimate at commit time | `cmd/kamish-broker/src/lib.rs:615` | Pre-allocation sizing is exact (`required_capacity(payload.len())`). If dynamic growth fails, WAL record remains durable with `RecordContext`. Reservation dropped. `repopulate_outbox_from_log` stages on queue drain. | R2, R14 | **None** |
| **A3** | Outbox capacity exhausted during restart repopulation | `cmd/kamish-broker/src/lib.rs:885` | `repopulate_outbox_from_log` intercepts `QueueCapacityExceeded`, halts iteration, and returns `Ok(staged_count)`. Staged batch replicates; subsequent refill stages remaining records. | R9, R14 | **None** |

---

## 2. Offset Gaps & Watermark Anomalies (Audit B)

| ID | Fault Injected | Code Location | System Response | Invariants Protected | Residual Risk |
|:---|:---|:---|:---|:---|:---|
| **B1** | Receiver receives contiguous next offset | `cmd/kamish-broker/src/lib.rs:756` | Appends record to log, advances partition watermark to `applied_offset + 1`, returns `accepted: true`. | R3, R5 | **None** |
| **B2** | Non-contiguous offset received (`from_offset > local_next`) | `cmd/kamish-broker/src/lib.rs:779` | Rejects with `accepted: false, reason: "offset_gap"`. Local watermark and log length remain unchanged. | R3, R4 | **None** |
| **B3** | Regressive or stale watermark received | `crates/kamish-outbox/src/lib.rs:198` | Watermark update ignored if `new_watermark <= current_watermark`. Trimming does not regress. | R5 | **None** |
| **B4** | Network retry sends duplicate record | `cmd/kamish-broker/src/lib.rs:734` | Receiver detects `to_offset <= local_next`. Checks payload, epoch, and CRC. Returns `accepted: true, reason: "idempotent_duplicate_noop"`. Log not duplicated. | R6 | **None** |
| **B5** | Network retry sends conflicting data for existing offset | `cmd/kamish-broker/src/lib.rs:741` | Receiver detects payload/CRC mismatch with historical record. Returns `accepted: false, reason: "replication_history_conflict"`. History preserved. | R7 | **None** |
| **B6** | Continuation of batch after gap error | `cmd/kamish-agent/src/lib.rs:705` | Agent logs warning but does not `break` outer loop; sends rest of batch, which fail with gap. | R4 | **Low (Efficiency)** |

---

## 3. Retry, Transport, and Scheduling Failures (Audit C)

| ID | Fault Injected | Code Location | System Response | Invariants Protected | Residual Risk |
|:---|:---|:---|:---|:---|:---|
| **C1** | Transport TCP connection drops mid-replication | `cmd/kamish-agent/src/lib.rs:880` | Client error captured; task retry scheduled up to `max_retries`. Outbox ranges retained. | R2, R10 | **None** |
| **C2** | Task exceeds 3 retries in `spawn_retry_worker` | `cmd/kamish-agent/src/lib.rs:655` | Task dropped from worker channel. Obligation preserved in WAL and outbox ranges. Stalls until external poll. | R10 | **Medium (Liveness)** |
| **C3** | Slow / degraded destination link | `crates/kamish-outbox/src/lib.rs:105` | Slow destination outbox fills to `max_bytes`. Independent destination outboxes continue operating. | R8 | **None** |
| **C4** | Shared multi-destination topic with one saturated destination | `cmd/kamish-broker/src/lib.rs:533` | Producer write rejected because saturated destination cannot reserve. Starves healthy destinations on shared topic. | R1 | **Medium (Fate Sharing)** |
| **C5** | Corrupt replication payload frame on wire | `cmd/kamish-broker/src/lib.rs:739` | CRC checksum verification fails; receiver rejects frame without appending to log. | R7, R12 | **None** |
| **C6** | Concurrent repopulation calls on same partition | `cmd/kamish-broker/src/lib.rs:836` | Partition-striped mutex (`partition_operations`) serializes recovery repopulation. | R1, R2, R9 | **None** |

---

## 4. Regional Outage & Restart Scenarios (Audit D)

| ID | Fault Injected | Code Location | System Response | Invariants Protected | Residual Risk |
|:---|:---|:---|:---|:---|:---|
| **D1** | Origin broker crashes with un-replicated WAL records | `cmd/kamish-broker/src/lib.rs:286` | On startup, `try_new` scans `data_dir`, opens partition logs, reads un-replicated records, and stages to outbox. | R9, R10 | **None** |
| **D2** | Destination broker crashes with partial prefix | `cmd/kamish-broker/src/lib.rs:756` | On recovery, origin resumes from destination's last acknowledged watermark. No duplicate or gap created. | R3, R6, R10 | **None** |
| **D3** | Complete loss of Region B while Region C healthy | `cmd/kamish-broker/tests/three_region_convergence_audit.rs` | Region C continues receiving replication for independent topics; Region B outbox saturates. | R8 | **None** |
| **D4** | Ephemeral metadata lease reset after restart | `crates/kamish-meta/src/lib.rs:604` | Prototype `MetadataStore` in-memory state is empty. Falls back to default epoch 1 unless re-registered. | R13 | **Low (Prototype)** |
| **D5** | Repeated saturation and drain cycles | `cmd/kamish-broker/tests/reservation_lifecycle_and_crash.rs` | Reservations accurately tracked; `queued_bytes` returns to exact 0 after drain. | R1, R2 | **None** |

---

## 5. Residency & Policy Boundary Failures (Audit E)

| ID | Fault Injected | Code Location | System Response | Invariants Protected | Residual Risk |
|:---|:---|:---|:---|:---|:---|
| **E1** | Destination revoked in policy before broker restart | `cmd/kamish-broker/src/lib.rs:880` | `repopulate_outbox_from_log` queries `Enforcer::decide(ReplicationSender)`. Rejection prevents staging to revoked destination. | R11 | **None** |
| **E2** | Producer specifies sealed ciphertext on unconfigured cluster | `cmd/kamish-broker/src/lib.rs:374` | Rejected with `PolicyDenied("sealed_replication_not_configured")`. Local append aborted. | R11 | **None** |
| **E3** | Receiver policy revoked mid-flight | `cmd/kamish-broker/src/lib.rs:710` | Receiver checks `AuditAction::ReplicationReceiver`. Rejection aborts append and returns `accepted: false`. | R11 | **None** |
| **E4** | Policy version rollback attempt | `crates/kamish-policy/src/lib.rs:277` | `PolicyCache` rejects policy with lower version than `highest_seen_version`. Current policy preserved. | R11 | **None** |

---

## 6. Storage & Append Faults (Audit F)

| ID | Fault Injected | Code Location | System Response | Invariants Protected | Residual Risk |
|:---|:---|:---|:---|:---|:---|
| **F1** | `sync_all` fails on record append | `crates/kamish-log/src/lib.rs:360` | Writer permanently fenced with `LogError::WriterFenced`. File handle dropped. Rollback attempted. | R12, R13 | **None** |
| **F2** | Torn record header written before crash | `crates/kamish-log/src/lib.rs:180` | On startup, log recovery reads up to first torn/invalid checksum and truncates partial record. | R12 | **None** |
| **F3** | Appending with mismatched producer epoch | `crates/kamish-log/src/lib.rs:325` | `FencedByEpoch` returned. Write rejected. | R13 | **None** |
| **F4** | Incomplete index file relative to log file | `crates/kamish-log/src/lib.rs:210` | Index rebuilt from data log scan during `PartitionLog::open`. Full consistency restored. | R12 | **None** |

