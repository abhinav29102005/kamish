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

---

## ADR-006: All-In-One Unified Packaging and Single-Command Global Distribution

**Status**: PROPOSED (Awaiting User Confirmation)  
**Date**: 2026-11-01  
**Deciders**: Platform Architecture & Core Engineering  

---

### 1. Context and Problem Statement
Deploying Kamish in distributed enterprise environments and local developer workflows previously required orchestrating independent Rust binaries (`kamish-broker`, `kamish-gateway`, `kamish-ctl`), managing a separate Node.js frontend web server, and configuring multi-process communication. To deliver a seamless developer onboarding experience without sacrificing performance or sovereignty guarantees, we evaluate an all-in-one unified packaging and global binary distribution model.

---

### 2. Architecture & Packaging Specifications

#### 2.1 Primary Global npm Package (`kamish`)
- Installed via `npm install -g kamish`, providing a single unified `kamish` command on the user's `PATH`.
- Supported subcommands:
  - `kamish start`: Launches the broker daemon, API gateway, and embedded dashboard concurrently in a single orchestrated process.
  - `kamish doctor`: Diagnoses system prerequisites, port availability, TLS certificates, etcd cluster quorum connectivity, and configuration validity with actionable error guidance.
  - `kamish init`: Interactive first-run setup initializing configuration skeletons and provisioning an initial administrator account interactively or from a secure credentials file (fails closed; zero default credentials ship).
  - `kamish topic [create|list]`: Topic creation and sovereign partition inspection.
  - `kamish produce` / `kamish fetch`: Record ingestion and consumption with cryptographic residency enforcement.
  - `kamish policy [sign|verify|publish]`: Ed25519 policy authoring, signature validation, and version floor management.
  - `kamish verify-audit`: Validates the local SHA-256 audit hash chain against consensus-witnessed checkpoints.
  - `kamish demo`: Executes scenario walkthroughs with explicit `[REAL]` vs `[SIMULATED]` provenance badges.

#### 2.2 Single Unified Binary with Embedded Dashboard
- Single platform-native Rust executable (`kamish`) compiled from a workspace binary crate combining `kamish-broker`, `kamish-gateway`, and `kamish-agent`.
- Built dashboard assets (`frontend/dist/`) are embedded directly into the binary at compile time using `rust-embed`.
- Embedded static assets are served over Axum with:
  - Strict `Content-Security-Policy` (`default-src 'self'`).
  - Long-term caching headers (`Cache-Control: public, max-age=31536000, immutable` for hashed assets).
  - Absolute local isolation: zero external network calls or third-party CDN fetches at runtime.

#### 2.3 Per-Platform Native npm Packages (`optionalDependencies`)
- Binary delivery leverages npm's platform-specific optional dependencies resolved by `os` and `cpu`:
  - `@kamish/kamish-linux-x64`
  - `@kamish/kamish-linux-arm64`
  - `@kamish/kamish-darwin-x64`
  - `@kamish/kamish-darwin-arm64`
  - `@kamish/kamish-win32-x64`
- Built in CI from pinned toolchains with SHA-256 integrity verification.
- **Offline Reliability**: Tarballs contain the full platform executable directly; zero post-install download scripts or curl invocations.

#### 2.4 Decoupled SDK Package (`@kamish/client`)
- `@kamish/client` remains a separate, lightweight TypeScript/Node.js SDK.
- The `kamish` CLI package may depend on `@kamish/client` for protocol communication, but the client SDK strictly never depends on the dashboard, Three.js, or native binaries.

#### 2.5 External Consensus Isolation & Tooling
- `etcd` is **never bundled** inside the Kamish binary.
- `kamish doctor` detects etcd connectivity and provides actionable guidance.
- Provides `docker-compose.etcd.yml` and a checksum-verified cluster bootstrap script (`scripts/start_etcd_cluster.sh`).
- Single-node compatibility mode operates completely without etcd, explicitly reporting `AUTHORITY: LOCAL (in-memory; quorum requires etcd)`.

#### 2.6 Fallback Distribution & Container Image
- Standalone platform binaries published to GitHub Releases accompanied by `SHA256SUMS`.
- Multi-arch Docker container (`kamish:latest`) based on minimal distroless/Alpine runtime.
- Publication stays strictly disabled until explicit written confirmation.

#### 2.7 Size Budget & Asset Strategy
- **Binary Size Budget**: < 35 MB compressed.
- **npm Wrapper Package**: < 50 KB.
- **3D Asset Management**: Lightweight default glTF models bundled into the binary; heavy uncompressed assets (> 50 MB) remain optional external downloads with documented CC0/MIT licenses in `LICENSE.md`.

#### 2.8 Dependency Control, Security & Licensing
- Strictly pinned package manifests (`Cargo.lock`, `package-lock.json`).
- Automated license auditing (`cargo-deny`, `license-checker`) and vulnerability scanning (`cargo-audit`, `npm audit`).
- Software Bill of Materials (SBOM) and bundled third-party credits documented in `NOTICE`.

#### 2.9 Clean-Room Verification Matrix
- Automated clean-room verification in temporary prefixes across Linux, macOS, and Windows.
- Tests verify: install -> `kamish --version` -> `kamish init` -> `kamish doctor` -> `kamish start` -> UI login -> produce/fetch -> clean shutdown -> uninstall.

#### 2.10 Publication Gate
- Read-only registry lookup performed: `kamish` and `@kamish/client` confirmed available (404 Not Found on npmjs.org).
- Stays strictly unpublished until explicit written approval.

