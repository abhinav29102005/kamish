# Milestone 3 — Epoch Fencing, Split-Brain Prevention & Quarantine Analysis

**Project:** HydraStream / Kamish Distributed Event Streaming Engine  
**Component:** `kamish-own`, `kamish-meta`, `kamish-log`  
**Invariants Evaluated:** `INV-7` (Epoch writer fencing & suffix quarantine)  
**Execution Date:** 2026-10-31  

---

## 1. Executive Summary

In distributed streaming engines, network partitions can cause two brokers to believe they are the authoritative owner of the same partition (split-brain). If an isolated former leader accepts writes without fencing, log history diverges, resulting in silent data loss or overwritten offsets.

HydraStream prevents split-brain corruption through a four-tier consensus and recovery architecture:
1. **Metadata Quorum Leases:** Ownership is granted via time-bounded leases recorded in `MetadataStore`.
2. **Epoch Monotonicity:** Ownership changes advance partition epochs monotonically via Compare-And-Swap (`compare_and_swap_owner`).
3. **Writer Fencing:** Any append stamped with an epoch lower than the metadata quorum's current epoch is rejected with `LogError::StaleEpoch`.
4. **Divergent Suffix Quarantine:** When a partitioned old leader reconnects, conflicting uncommitted local records are moved into `QuarantineStore`, preserving forensic evidence without polluting the linear log.

---

## 2. Ownership Lifecycle & Epoch Progression

```mermaid
sequenceDiagram
    autonumber
    participant Follower as Broker B (Follower)
    participant Quorum as Metadata Quorum
    participant OldLeader as Broker A (Old Leader)
    participant Quarantine as QuarantineStore

    Note over OldLeader: Network partition isolates Broker A
    OldLeader->>OldLeader: Lease expires (now > lease_expiry_ns)
    Follower->>Quorum: try_promote(topic, partition, current_epoch=1)
    Quorum->>Quorum: CAS(epoch: 1 -> 2, owner: Broker B)
    Quorum-->>Follower: Promotion Granted (epoch=2)
    
    Note over OldLeader: Stale writer attempts append with epoch 1
    OldLeader->>OldLeader: append(epoch=1) -> REJECTED (StaleEpoch)
    
    Note over OldLeader: Network heals; Old Leader reconnects
    OldLeader->>Quorum: Read current ownership (owner=B, epoch=2)
    OldLeader->>Follower: Handshake last agreed watermark (offset=100)
    OldLeader->>OldLeader: Identify divergent local suffix [101..103]
    OldLeader->>Quarantine: Move records [101..103] to QuarantineStore
    OldLeader->>OldLeader: Truncate log to agreed offset 100
    OldLeader->>Follower: Catch up as follower from Broker B
```

---

## 3. Deep Analysis of Epoch Fencing Properties

### 3.1 Who Creates and Advances Epochs?
Epochs are governed by the metadata store (`crates/kamish-meta/src/lib.rs`):
```rust
pub fn compare_and_swap_owner(
    &self,
    topic: &str,
    partition: u32,
    expected_epoch: u64,
    new_owner: &str,
    lease_duration_ms: i64,
) -> Result<PartitionOwnership, MetaError>
```
- A new epoch can only be established if `expected_epoch == current_epoch`.
- When successful, `new_epoch = expected_epoch + 1`.
- The new lease duration is stamped into `lease_expiry_ns`.

### 3.2 Split-Brain Prevention: Active Lease Fencing
Before a backup broker can promote itself to leader:
1. It queries `MetadataStore::get_ownership(topic, partition)`.
2. It verifies that the current lease has expired (`now_ns > lease_expiry_ns`).
3. If an aspiring leader attempts promotion while the incumbent leader's lease is active:
   `OwnershipManager::try_promote` fails with `OwnershipError::LeaseNotExpired`.
- *Verification:* Verified by `test_promotion_before_lease_expires_fails`.

### 3.3 Stale Writer Fencing at the Storage Layer
In `PartitionLog::append` (`crates/kamish-log/src/lib.rs`):
```rust
let mut inner = self.inner.write().unwrap();
if epoch < inner.current_epoch {
    return Err(LogError::StaleEpoch {
        current: inner.current_epoch,
        requested: epoch,
    });
}
if epoch > inner.current_epoch {
    inner.current_epoch = epoch;
}
```
If an old leader process attempts to write after its partition has been promoted to epoch $E_{new}$, any write with $E_{stale} < E_{new}$ is rejected immediately at the disk append boundary.

---

## 4. Divergent Suffix Quarantine Architecture

When a returning old leader discovers its log has diverged from the authoritative leader:
1. **Identification:** The returning broker reads the authoritative leader's watermark $W_{auth}$.
2. If the local log contains records where `base_offset >= W_auth` with a lower epoch:
   `OwnershipManager::reconcile_divergent_suffix` extracts the divergent slice.
3. **Forensic Preservation:** The records are appended into `QuarantineStore` (`crates/kamish-own/src/lib.rs`):
   - The file is saved as `quarantine/{topic}_{partition}_epoch_{epoch}.data`.
   - Records are NOT deleted, discarded, or overwritten.
   - Operators can inspect quarantined payloads for compliance, auditing, and reconciliation.
4. **Log Truncation & Continuity:**
   The partition segment file is truncated back to $W_{auth}$. The returning broker resumes replication catch-up as a follower from offset $W_{auth}$.
- *Verification:* Verified by `test_quarantine_divergent_suffix_on_split_brain` and `test_quarantine_superseded_records`.

