# KAMISH MILESTONE 5.4: DESIGN OPTIONS EVALUATION
## Architectural Comparison of Metadata Ownership & Fencing Strategies

**Repository:** `E:\Kamish`  
**Milestone:** 5.4 Metadata Ownership Durability & Epoch Fencing Audit  
**Scope:** Evaluation of architectural paths for partition ownership, lease management, and cross-region coordination.

---

## 1. Context & Problem Statement

Kamish partitions require strong ownership guarantees:
1. Exactly one broker region may actively accept writes for a partition at a given epoch.
2. Ownership transitions must monotonically advance the epoch fence.
3. Writers operating under a revoked or expired lease must be fenced before committing disk modifications.
4. Storage engines must not mistake local log recovery for authoritative cluster ownership.

Three design alternatives are evaluated below.

---

## 2. Option A: Persistent Local Metadata State

### 2.1 Architecture
In Option A, each broker persists its metadata state (registered streams, ownership leases, and assigned epochs) to an embedded, crash-consistent local key-value store (e.g., SQLite, sled, or an append-only JSON/binary metadata log) alongside the partition data directory.

```
+-------------------------------------------------------------+
|                      Broker Node                            |
|                                                             |
|  +--------------------+        +-------------------------+  |
|  |   PartitionLog     |        |   Local Meta Store      |  |
|  |  (partition.data)  |        |    (meta.db / sled)     |  |
|  +--------------------+        +-------------------------+  |
|           ^                                 ^               |
|           |                                 |               |
|  +-------------------------------------------------------+  |
|  |             BrokerServer Produce Loop                 |  |
|  +-------------------------------------------------------+  |
+-------------------------------------------------------------+
```

### 2.2 Assumptions
- Nodes operate with independent, locally durable metadata files.
- Cross-node coordination requires external point-to-point messaging or administrative RPCs.
- Local disk writes are durable after `sync_all`.

### 2.3 Failure Modes & Edge Cases
- **Split-Brain Risk:** If two broker instances in different regions each maintain local metadata files, nothing prevents both nodes from declaring themselves owner of `(events, 0)` at epoch 1 simultaneously unless an out-of-band lock is acquired.
- **Asymmetric Crash Consistency:** If a crash occurs between the local metadata update and the WAL append, the node must reconcile whether the lease or epoch was committed.

### 2.4 Recovery Behavior
- On restart, the broker reopens `meta.db`, recovers its last known assigned epoch and lease timestamp, and cross-checks with `PartitionLog::open`.
- If the lease has expired during downtime, the broker marks the partition unowned until an administrative command or election reassignment occurs.

### 2.5 Trade-offs & Scope
- **Pros:** Zero external dependencies; zero network hops on produce path; simple single-binary deployment.
- **Cons:** Cannot solve multi-region split-brain without external consensus; failover requires complex manual or split-brain-vulnerable peer-to-peer voting.

---

## 3. Option B: Consensus-Backed Ownership Authority (etcd / Raft)

### 3.1 Architecture (Implemented in `EtcdMetadataAuthority`)
In Option B, an external, strongly consistent distributed consensus quorum (`etcd 3.7.3` or an embedded Raft group) acts as the single source of truth for partition ownership and epoch progression.

```
+------------------+                    +------------------+
|  Broker US-EAST  |                    |  Broker EU-WEST  |
+--------+---------+                    +--------+---------+
         |                                       |
         | mTLS (CAS / Lease Grant)              | mTLS (CAS / Lease Grant)
         v                                       v
+----------------------------------------------------------+
|          Authoritative Consensus Quorum (etcd 3.7.3)      |
|                                                          |
|  - Epoch Key: kamish/streams/{topic}/{p}/epoch (Durable) |
|  - Owner Key: kamish/streams/{topic}/{p}/owner (Leased)  |
|  - Policy Floor: kamish/clusters/{id}/policy/floor       |
|  - Witness Checkpoints: kamish/audit/streams/{topic}/{p} |
+----------------------------------------------------------+
```

### 3.2 Assumptions
- An odd number of consensus nodes (typically 3 or 5) spanning availability zones or regions.
- Time-bounded network latency between brokers and consensus nodes.
- Pure-Rust TLS with mutual authentication (mTLS) for secure cross-region RPCs.
- Broker nodes maintain monotonic local clocks for lease validity tracking.

### 3.3 Failure Modes & Edge Cases
- **Consensus Partition (Quorum Loss):** If network partitions isolate nodes such that < 2/3 members are reachable, the quorum fails closed. Write admissions halt until quorum heals.
- **RPC Overhead on Write Path:** If `get_ownership` queries etcd per produce call (current Gap 1), local produce latency is coupled to consensus network latency.
- **Takeover During Active Lease:** If CAS does not verify that the prior owner's lease has expired plus drift buffer (current Gap 3), a forced takeover could cause split-brain ACKs.

### 3.4 Recovery Behavior
- When an owner crashes, its etcd lease expires after TTL. The epoch key remains intact. A candidate node calls `try_promote`, waits for the lease expiry margin, and executes CAS `E -> E+1`.
- If the whole etcd cluster is cold-restarted, all committed epochs and policy floors persist on disk and are recovered by Raft without regression.

### 3.5 Trade-offs & Scope
- **Pros:** Mathematically sound linearizability; solves split-brain across regions; robust cold-restart persistence; decoupled storage and coordination.
- **Cons:** Requires running and operating an etcd cluster (or embedded Raft); requires careful caching in the broker to avoid violating NFR-01.

---

## 4. Option C: Existing Prototype Limitations (Explicit Single-Node Boundary)

### 4.1 Architecture
In Option C, the system retains the in-memory `MetadataStore`, but explicitly bounds its guarantees:
- `SingleNodeCompatibility` mode is officially documented and marked as **Local Simulation Only**.
- Multi-node and multi-region deployments are forbidden without configuring `EtcdMetadataAuthority`.
- The broker emits a startup warning when running with volatile in-memory metadata.

### 4.2 Assumptions
- Used solely for single-process developer testing, CI unit tests, and local benchmarking.
- Fault tolerance is limited to local WAL torn-write and fsync recovery.

### 4.3 Failure Modes & Edge Cases
- Process restart drops all metadata leases and ownership mappings.
- Multi-broker concurrency is completely undefined.

### 4.4 Trade-offs & Scope
- **Pros:** Zero operational overhead; instant test execution; no external processes needed.
- **Cons:** Does not satisfy production multi-region hackathon resilience or cross-region failover claims.

---

## 5. Architectural Recommendation

The optimal path forward is **Option B with targeted remediation of Gaps 1, 2, and 3**:

1. **Retain `EtcdMetadataAuthority` as the production multi-region backend:** Commit `27cbc49` has already implemented the pure-Rust TLS, atomic CAS Txn, and persistent epoch key mechanisms.
2. **Remediate Gap 1 (Local Lease Caching on Produce Path):** Modify `BrokerServer::produce` to evaluate the cached `LocalLease` directly without performing a blocking network KV read to etcd, satisfying NFR-01.
3. **Remediate Gap 2 (Monotonic Clock in Broker Loop):** Pass `Arc<dyn MonotonicClock>` into `BrokerServer` and use `clock.now_monotonic_ms() < lease.monotonic_expiry_ms` for pre- and post-append fencing.
4. **Remediate Gap 3 (Enforce Lease Expiry Buffer in CAS):** Ensure all broker ownership claims route through `PartitionOwnerCoordinator::try_promote` so that no epoch bump can succeed while a prior owner's physical lease + drift margin is active.
5. **Retain `MetadataStore` strictly for single-node isolated unit testing.**

