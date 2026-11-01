# Multi-Region Durability & Replication Baselines (W32)

This directory documents the version-pinned configuration and topology manifests for evaluating Kamish against enterprise streaming platforms on an identical 3-region topology.

## 1. Controlled Topology
- **Regions:** `EU_WEST` (Dublin), `US_EAST` (N. Virginia), `AP_SOUTH` (Mumbai).
- **Network Profile:**
  - EU <-> US: ~72ms RTT
  - US <-> AP: ~185ms RTT
  - EU <-> AP: ~115ms RTT

## 2. Invariant & Durability Equivalence
To ensure fair comparison, all candidate systems are configured with equivalent production durability:
- **In-Region Persistence Quorum:** Exactly 3 replicas in each region.
- **Producer Acknowledgment:** ACK emitted strictly after a majority (at least 2 of 3) persist to durable storage.
- **Unclean Leader Election:** Strictly disabled (`unclean.leader.election.enable=false`) to eliminate uncommitted split-brain history.
- **Zero Downgrades:** Neither Kafka nor Pulsar is tuned down or run in single-node/in-memory modes.

## 3. Pinned System Versions
| System | Version | Module |
| --- | --- | --- |
| **Kamish** | `0.1.0` (commit HEAD) | Native Broker + Outbox Worker |
| **Apache Kafka** | `3.9.0` | Kafka Broker (KRaft mode) + MirrorMaker 2 |
| **Apache Pulsar** | `4.0.0` | Pulsar Broker + BookKeeper Ensemble (3 Bookies) |
