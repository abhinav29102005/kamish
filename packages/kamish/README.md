# kamish

Unified command-line interface, sovereign event streaming broker, and embedded operations dashboard for Kamish.

## Overview

`kamish` is the single global entry point for developing, operating, and auditing sovereign event streaming topologies. It embeds:
- The regional sovereign broker daemon (sub-millisecond local ingestion, append-only WAL, outbox isolation).
- The REST/SSE API gateway.
- The web management dashboard with real-time audit chain verification and telemetry.

## Installation

```bash
npm install -g kamish
```

Or run instantly without installation using `npx`:

```bash
npx kamish start --dev
```

### Platform Support

| Operating System | Architecture | Launcher Support | Execution Engine |
| :--- | :--- | :--- | :--- |
| **Windows** | x64, ARM64 | CMD (`kamish.cmd`), PowerShell (`kamish.ps1`), WSL | Native `kamish.exe` or Portable Node Engine |
| **macOS** | Apple Silicon (`arm64`), Intel (`x64`) | Zsh, Bash | Native Mach-O or Portable Node Engine |
| **Linux** | x86_64, aarch64 | Bash, Sh | Native ELF (12.5MB) or Portable Node Engine |

When installed via npm, native binaries are resolved via platform-specific `optionalDependencies` with zero post-install download scripts. If native prebuilt binaries are unavailable, `kamish` seamlessly runs using its zero-dependency portable engine with full REST/SSE APIs, cryptographic audit verification, multi-topic event streaming, and embedded dashboard serving.

## Quickstart

### 1. Initialize Cluster Skeleton
```bash
kamish init
```
Generates configuration (`kamish.toml`), provisions an administrator account interactively or from a secret file, generates Ed25519 signing keys, and authors a default residency policy.

### 2. Verify System Prerequisites
```bash
kamish doctor
```
Diagnoses system readiness, port availability, consensus quorum connectivity, and configuration validity with actionable remediation advice.

### 3. Launch Unified Broker & Dashboard
```bash
kamish start
```
Spins up the broker daemon, REST/SSE gateway, and embedded web dashboard in a single orchestrated process at `http://127.0.0.1:3001`.

## Subcommands

- `kamish start`: Launch broker, gateway, and dashboard in one process.
- `kamish doctor`: Validate ports, versions, consensus reachability, and configuration.
- `kamish init`: Interactive first-run setup initializing configuration skeletons.
- `kamish status`: Query cluster state, authority consensus model, and active leases.
- `kamish topic create|list`: Create and inspect sovereign topics.
- `kamish produce`: Produce records with residency policy enforcement.
- `kamish consume|fetch`: Read records from partition offsets.
- `kamish policy sign|verify|publish`: Author, cryptographically verify, and publish Ed25519 policies.
- `kamish verify-audit`: Verify cryptographic hash-chain integrity of audit logs.
- `kamish benchmark`: Run ingestion and calibrated network impairment benchmarks.
- `kamish demo`: Execute scenario walkthrough with explicit REAL vs SIMULATED badges.
- `kamish completion <shell>`: Generate shell completions for bash, zsh, fish, PowerShell.

## Limitations & Honesty Disclosures

- **Consensus & Multi-Region**: Single-node mode runs without `etcd` using local fsync storage (`LOCAL_PERSISTENT_STORAGE`). Multi-region CAS ownership, lease fencing, and partition promotion require an active `etcd` cluster.
- **Experimental Alpha**: This is release `0.1.0-alpha.1` intended for local evaluation and research validation. Production use requires independent cryptographic and operational audit.
- **What We Do NOT Claim**:
  - We do NOT claim zero-loss without quorum-backed replication.
  - We do NOT claim drop-in Kafka protocol wire equivalence.
  - We do NOT claim hardware-level enclave sealing for unencrypted local disk logs.

## License

Apache-2.0
