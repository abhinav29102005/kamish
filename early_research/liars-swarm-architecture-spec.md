# Decentralized Computation Verification: Core Diagrams & System Specification

A comprehensive architectural specification for **Liar's Swarm**, defining the sequence workflows, data flow diagrams (DFD), implementation roadmap, and modular 5-plane layering for decentralized compute verification under Byzantine fault conditions (up to 25% malicious participants).

---

## 1. Modular System Architecture & Layer Separation

The Liar's Swarm architecture decouples consensus from computation verification across five distinct operational planes. Consensus establishes agreement on state transitions and ordering, while verification proves that off-chain computation outputs match actual deterministic execution.

| Architecture Plane | Primary Responsibilities | Key Technologies & Components |
| :--- | :--- | :--- |
| **1. Peer Plane** | Identity management, peer discovery, connection handling, and message propagation. | libp2p, Kademlia DHT, GossipSub, Secio/TLS |
| **2. Task Plane** | Task manifest canonicalization, resource quota binding, VRF committee scheduling, and deadline enforcement. | Canonical Task Manifest (Protobuf), VRF Randomness, Execution Diversity Rules |
| **3. Execution Plane** | Sandboxed execution, resource isolation, and bit-for-bit deterministic runtime environment. | WASM / WASI runtime (Wasmtime), Floating-Point Guards, Memory/CPU Quotas |
| **4. Verification Plane** | Adaptive redundancy control, spot audits, challenge protocols, and result commitment matching. | 1x–3x Dynamic Redundancy Engine, Probabilistic Audit Sampler, Challenge Arbiter |
| **5. Agreement Plane** | BFT state logging, quorum validation, reward/slashing distribution, and receipt issuance. | BFT Consensus Log ($N \ge 3f + 1$), Audit Trail Ledger, Signed Receipts |

---

## 2. Core Sequence Diagrams

### 2.1 Primary Task Lifecycle & Adaptive Verification Sequence

The primary sequence governs the lifecycle of a task from initial submission by a client to final verified state commitment and receipt issuance.

#### Stepwise Execution Sequence

1. **Task Manifest Binding**: The Task Issuer packages task code, input datasets, execution resource caps, and verification policy into a canonical manifest. The manifest generates a cryptographic manifest hash $H_{\text{manifest}} = \text{SHA256}(\text{Code} \parallel \text{Inputs} \parallel \text{Policy})$.
2. **Peer Discovery & VRF Committee Selection**: The network uses verifiable random functions (VRF) seeded with epoch entropy to select a worker committee. Selection rules enforce hardware, operator, and runtime diversity to prevent correlated collusion.
3. **Dispatch & WASI Sandboxed Execution**: Selected worker nodes download the WASM module and inputs, executing the job within a deterministic WASI sandbox enforcing strict memory, CPU, and wall-clock bounds.
4. **Commitment Submission**: Worker nodes output an encrypted/hashed result commitment $C_i = \text{SHA256}(\text{Result}_i \parallel \text{Salt}_i)$ and broadcast it to the GossipSub network.
5. **Adaptive Verification & Redundancy Check**: The Verification Engine evaluates matching commitments:
   * **Fast Path (1x Redundancy)**: If node risk score is low and no spot-check is triggered, the single result commitment is provisionally accepted.
   * **Consensus Path (3x Redundancy)**: If disagreement occurs or the task is high-value, redundancy automatically scales to 3x execution.
   * **Audit Path**: A random audit challenge is dispatched to a randomly selected auditor node.
6. **BFT Quorum Commit**: Once quorum criteria ($N \ge 3f + 1$) are met, state transitions are committed to the BFT log.
7. **Receipt Delivery**: A cryptographically signed audit receipt containing the verified output hash, execution log, and rewards/penalties is delivered to the Task Issuer.

#### Sequence Matrix & State Transitions

| Phase | Origin | Target | Message / Payload | Verification / Constraint |
| :--- | :--- | :--- | :--- | :--- |
| **Submission** | Client | Peer Router | `TaskManifestSpec` | Validates signatures & resource deposit |
| **Scheduling** | VRF Engine | Compute Peers | `CommitteeAssignMsg` | Enforces operator & hardware diversity |
| **Execution** | Worker Node | WASI Sandbox | `ExecWasmJob(CodeHash)` | Enforces bit-deterministic WASM profile |
| **Commitment** | Worker Node | GossipSub | `CommitHash(TaskID, C_i)` | Prevents front-running & answer copying |
| **Evaluation** | Verifier Engine | Consensus Log | `VerificationDecision` | Evaluates $H_{\text{out}}$ consensus ($2f+1$) |
| **Finalization** | BFT Ledger | Client | `SignedStateReceipt` | Provides non-repudiable proof of state |

---

### 2.2 Challenge-Response & Probabilistic Audit Sequence

When result commitments disagree or a probabilistic spot-check is triggered, the system invokes the challenge-response protocol.

```
Client / Issuer              Worker Node A            Worker Node B (Challenger)         Dispute Arbiter (BFT)
      |                            |                               |                                 |
      |--- Submit Result Hash ---->|                               |                                 |
      |                            |--- Commit Result (Hash A) --->|                                 |
      |                            |                               |--- Detect Hash Discrepancy ---->|
      |                            |                               |<-- Issue Challenge Request -----|
      |                            |<-- Request Execution Evidence-|                                 |
      |                            |--- Submit WASM Memory Trace ->|                                 |
      |                            |                               |--- Re-execute in Ref Sandbox -->|
      |                            |                               |--- Submit Audit Proof --------->|
      |                            |                               |                                 |--- Evaluate Evidence
      |                            |                               |                                 |--- Slash Malicious Node A
      |<---------------------------|-------------------------------|---------------------------------|--- Commit Verified Result
```

---

## 3. Data Flow Diagrams (DFD)

### 3.1 Level 0 Context Data Flow Diagram

The Context DFD defines the boundary between external entities (Task Issuers, P2P Compute Workers, and External Auditors) and the Liar's Swarm Verification Engine.

* **External Inputs**:
  * Task Specifications, WASM Binaries, and Execution Quotas (Task Issuer -> System).
  * Worker Node Capacity Certificates and VRF Public Keys (P2P Compute Workers -> System).
  * Dispute Challenges and Verification Audits (External Auditors -> System).
* **System Outputs**:
  * Signed Execution Receipts and Verified Computation Outputs (System -> Task Issuer).
  * Task Assignments, Resource Claims, and Reward Allocations (System -> P2P Compute Workers).
  * Slashing Logs and Audit Trail Ledger (System -> External Auditors).

---

### 3.2 Level 1 Detailed Architecture Data Flow Diagram

The Level 1 DFD decomposes the system into five core processes and four durable data stores.

```
+------------------+         Task Request        +-----------------------------------+        Store Manifest        +-------------------------+
|                  |---------------------------->| 2.0 Manifest & Resource Quota     |----------------------------->| D2: Task Manifest Store |
|   Task Issuer    |                             | Binding                           |                              +-------------------------+
| (External Entity)|<----------------------------+-----------------------------------+                                           |
+------------------+     Signed State Receipt                      |                                                         |
         ^                                                         | Task Manifest                                           |
         |                                                         v                                                         v
         |                                       +-----------------------------------+     Peer List & VRF Seeds    +-------------------------+
         |                                       | 1.0 Peer Discovery & VRF          |<-----------------------------| D1: Peer DHT Registry   |
         |                                       | Committee Selection               |                              | & VRF State             |
         |                                       +-----------------------------------+                              +-------------------------+
         |                                                         |                                                             ^
         |                                                         | Assign WASM Task                                            |
         |                                                         v                                                             |
         |                                       +-----------------------------------+                                           |
         |                                       | P2P Worker Nodes (Compute Peers)  |                                           |
         |                                       +-----------------------------------+                                           |
         |                                                         |                                                             |
         |                                                         | Execute WASM Code                                           |
         |                                                         v                                                             |
         |                                       +-----------------------------------+     Commit Result Hash       +-------------------------+
         |                                       | 3.0 Deterministic WASM Sandbox    |----------------------------->| D3: Result Commitment   |
         |                                       | Execution                         |                              | Log                     |
         |                                       +-----------------------------------+                              +-------------------------+
         |                                                                                                                       |
         |                                                                                                                       | Fetch Commitments
         |                                       +-----------------------------------+                                           v
         |                                       | 4.0 Adaptive Verification & Spot  |<--------------------------------------------------+
         |                                       | Audit Engine                      |                                                   |
         |                                       +-----------------------------------+                                                   |
         |                                                         |                                                                     |
         |                                                         | Verified State                                                      |
         |                                                         v                                                                     |
         |                                       +-----------------------------------+       Update Audit Trail     +-------------------------+
         +---------------------------------------| 5.0 BFT Consensus & Receipt       |----------------------------->| D4: BFT State Ledger    |
                                                 | Logging                           |                              | & Audit Trail           |
                                                 +-----------------------------------+                              +-------------------------+
```

#### Process & Data Store Reference Matrix

| Process / Store ID | Name | Inputs | Outputs | Primary Function |
| :--- | :--- | :--- | :--- | :--- |
| **Process 1.0** | Peer Discovery & VRF Selection | VRF Seeds, Peer Registry (D1) | Task Assignment Messages | Selects diverse execution committees via VRF. |
| **Process 2.0** | Manifest & Quota Binding | Raw Job Spec from Issuer | Manifest Entry (D2) | Binds canonical code, input, and resource rules. |
| **Process 3.0** | Deterministic WASM Execution | Manifest Entry (D2), Worker Code | Execution Commitment (D3) | Runs WASM binary with strict float/clock traps. |
| **Process 4.0** | Adaptive Verification Engine | Commitments (D3), Audit Challenges | Verified Output State | Evaluates 1x–3x consistency & random audits. |
| **Process 5.0** | BFT Consensus & State Logging | Verified State | State Ledger (D4), Receipts | Logs state transitions and issues signed receipts. |
| **Store D1** | Peer DHT Registry | Node Registrations, VRF Keys | Active Peer Membership | Maintains Kademlia DHT routing table & peer risk. |
| **Store D2** | Task Manifest Store | Canonical Specs | Job Configurations | Stores immutable task requirements and code hashes. |
| **Store D3** | Result Commitment Log | Worker Hash Commitments | Commitment Pairs | Stores double-hashed execution outputs ($C_i$). |
| **Store D4** | BFT Ledger & Audit Trail | Verified Quorum State | Receipts & Slashing Logs | Provides durable append-only log of finalized jobs. |

---

## 4. Implementation Chart & 6-Stage Engineering Roadmap

To demonstrate technical depth and deliver a winning hackathon prototype, development follows a staged six-phase sequence from deterministic simulation to benchmark reporting.

```
+----------------------------------------------------------------------------------------------------+
| STAGE 1: Event-Driven Simulation Harness                                                           |
| • Build discrete-event simulator testing 4 to 100 peers under 0% to 25%+ Byzantine fault load.     |
| • Implement fault injectors: lying outputs, task stalling, colluding quorums, and Sybil nodes.    |
| • Deliverables: Quantitative metric collector (FAR, FRR, completion latency, compute overhead).    |
+----------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+----------------------------------------------------------------------------------------------------+
| STAGE 2: P2P Networking & Identity Layer                                                           |
| • Implement libp2p node stack with Secio/TLS peer identity and Kademlia DHT discovery.             |
| • Configure GossipSub topics for task distribution, hash commitments, and audit challenges.        |
| • Deliverables: Multi-node local P2P cluster broadcasting signed task specifications.             |
+----------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+----------------------------------------------------------------------------------------------------+
| STAGE 3: Sandboxed Execution Engine                                                                |
| • Integrate Wasmtime WASM/WASI runtime with strict execution isolation.                            |
| • Disable non-deterministic hazards: floating-point non-determinism, host clocks, random seeds.   |
| • Deliverables: Bit-for-bit reproducible execution across heterogeneous host environments.          |
+----------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+----------------------------------------------------------------------------------------------------+
| STAGE 4: Adaptive Verification & Audit Engine                                                      |
| • Implement dynamic redundancy controller (1x baseline scaling to 3x on discrepancy or risk).      |
| • Build spot-checking sampler and challenge-response dispute arbiter.                              |
| • Deliverables: Verification engine detecting colluding lies with high probability (P_detect).     |
+----------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+----------------------------------------------------------------------------------------------------+
| STAGE 5: BFT Consensus & State Recovery                                                            |
| • Integrate PBFT/Tendermint-style state log for committing state transitions (N >= 3f + 1).        |
| • Implement automated failure detection and task reassignment for stalled/timeout nodes.           |
| • Deliverables: Liveness and safety preservation during worker dropouts and 25% malicious load.    |
+----------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+----------------------------------------------------------------------------------------------------+
| STAGE 6: Evaluation & Hackathon Benchmarking                                                       |
| • Execute benchmark test matrix comparing baseline 3x fixed replication vs. Adaptive Redundancy.   |
| • Generate benchmark plots for False Acceptance Rate (FAR) vs. Adversarial Fraction.               |
| • Deliverables: Reproducible benchmark scripts, threat model paper, and live P2P terminal demo.   |
+----------------------------------------------------------------------------------------------------+
```

---

## 5. Quantitative Metric & Verification Target Matrix

| Metric Category | Target Value / Limit | Evaluation Formula / Mechanism | Architectural Defense |
| :--- | :--- | :--- | :--- |
| **False Acceptance Rate (FAR)** | $< 0.01\%$ under 25% Collusion | $\text{FAR} = \frac{\text{Incorrect Accepted Outputs}}{\text{Total Malicious Submissions}}$ | VRF Committee Diversity + Spot Audits |
| **False Rejection Rate (FRR)** | $< 0.1\%$ under Network Jitter | $\text{FRR} = \frac{\text{Valid Rejected Outputs}}{\text{Total Honest Submissions}}$ | WASI Deterministic Floating-Point Guards |
| **Compute Overhead Savings** | $40\% - 65\%$ vs Fixed 3x | $\text{Savings} = 1 - \frac{\text{Adaptive Compute Hours}}{\text{Fixed 3x Compute Hours}}$ | Adaptive Redundancy (1x default, 3x fallback) |
| **Task Completion Latency** | P95 $< 2.5\times$ Single Execution | $T_{\text{P95}} = \text{Time}_{\text{Commit}} - \text{Time}_{\text{Submission}}$ | Parallel WASM Execution & GossipSub |
| **Sybil Resistance Threshold** | Max 25% Voting Weight | Cryptographic VRF + Staking / Resource Certificates | Committee Selection Diversity Rules |

---

## 6. Summary of Published Visual Assets

The architectural diagrams generated for this specification are available as standalone visual assets:

1. **`sequence_diagram.png`**: Visual flowchart detailing the task lifecycle from submission to signed receipt.
2. **`dfd_level1_diagram.png`**: Level 1 Data Flow Diagram mapping internal processes and durable data stores.
3. **`implementation_roadmap_chart.png`**: 6-Stage System Implementation Chart outlining the build phase progression.
