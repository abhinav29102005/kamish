# Architecture Decision Record: Broker Process Topology & Failure Isolation

## ADR-005: Embedded Broker vs. Standalone Broker Process

**Status**: PROPOSED (Awaiting User / Architecture Choice)  
**Date**: 2026-11-01  
**Deciders**: Engineering & Platform Architecture  

---

### 1. Context and Problem Statement
In the current Phase 5 prototype implementation, the regional `BrokerServer` runs in-process, directly embedded within the `kamish-gateway` daemon process. The gateway holds an `Arc<BrokerServer>` and dispatches requests in-memory.

While this architecture minimizes serialization latency and simplifies single-binary developer ergonomics, it couples the edge API surface with the stateful storage and replication engine. We evaluate two architectural topologies for production deployment:
- **Option (a)**: Standalone dedicated broker daemon process accessed by the gateway via authenticated mTLS/gRPC or Unix domain socket client.
- **Option (b)**: Dual-mode architecture retaining embedded mode for local development and integration tests, while introducing a standalone decoupled process mode for staging and multi-region production.

---

### 2. Comprehensive Options and Trade-Off Analysis

| Dimension | Option (a): Standalone Broker Process | Option (b): Dual-Mode (Embedded Dev + Standalone Prod) | Current State (Pure Embedded) |
| :--- | :--- | :--- | :--- |
| **Failure Isolation** | **Complete**: Gateway OOM or panic cannot corrupt broker storage; broker crash leaves gateway running to serve maintenance/503 pages. | **Complete in Production**: Dev uses shared memory; production enforces process isolation boundaries. | **Zero**: Uncaught panic or edge OOM in gateway terminates stateful broker partitions. |
| **Chaos Scenarios** | **Realistic**: Broker crash scenarios (Scenario C) kill the actual broker PID, exercising real client reconnection and metadata failover. | **Realistic in Prod**: Integration tests can test both in-memory and external broker process kill/SIGKILL. | **Simulated**: Scenario C must simulate broker outage in memory or restart entire gateway binary. |
| **Network & IPC Overhead** | ~0.15ms local IPC / UDS overhead per produce/fetch request. | Zero overhead in dev; ~0.15ms in production. | **Zero**: In-memory Rust function calls over `Arc`. |
| **Deployment Complexity** | Requires supervising two processes (`kamish-broker` + `kamish-gateway`) via systemd/supervisord or separate containers. | Single binary in development; orchestrated containers in production. | Single binary everywhere. |
| **Maintenance Burden** | Moderate: Broker API client must be maintained with connection pooling and circuit breaking. | Higher: Dual dispatch path (in-memory `Arc<BrokerServer>` vs `BrokerClient`) must be tested and maintained. | Lowest: Single code path. |

---

### 3. Deep-Dive on Failure Isolation & Chaos Scenarios

#### 3.1 Failure Isolation Impact
1. **Memory & Resource Exhaustion (OOM)**:
   - In the embedded model, if an external client floods the gateway with large unauthenticated JSON payloads or unbounded SSE subscriptions, the OS OOM killer may terminate the gateway process mid-flush. This poses a risk of un-synced dirty pages in active log segments.
   - Under Option (a) and Option (b) prod mode, the broker runs in an independent cgroup with strict memory reservation (e.g. 4GB for log cache and outboxes). The edge gateway runs in a separate, lower-priority cgroup that can be throttled or restarted without interrupting partition consensus leases or replication catch-up.
2. **Panic Boundary**:
   - A deserialization bug or panicking handler in the gateway web framework (Axum) cannot tear down the raft/lease renewal loop or drop active WAN replication streams.

#### 3.2 Impact on Chaos and Outage Scenarios
1. **Scenario A (Residency Violation)**:
   - Evaluated identically in both modes via policy enforcer.
2. **Scenario B (Slow WAN Link & Backpressure)**:
   - Standalone broker enables real network interface traffic shaping (`tc netem delay 200ms`) between broker instances without artificially throttling gateway REST callers.
3. **Scenario C (Regional Outage & Partition Failover)**:
   - In standalone mode, regional failover tests send `SIGKILL` to the broker PID. The gateway detects peer disconnection via client heartbeat loss, re-routes fetch requests to backup replicas, and reports honest `BROKER_UNREACHABLE` status instead of terminating the UI.

---

### 4. Recommendation for Architecture Selection
- **Option (a)** is recommended if strict production readiness, containerized Kubernetes microservices, and absolute process failure isolation are paramount.
- **Option (b)** is recommended if rapid local development, single-command onboarding (`cargo run`), and embedded CI test speed are desired alongside production isolation.

*Status: Awaiting user confirmation to proceed with either Option (a) or Option (b).*
