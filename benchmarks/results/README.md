# Kamish Benchmark Evidence Archives

This directory stores machine-readable benchmark run manifests, execution telemetry, and baseline comparisons across Kamish, Apache Kafka (MirrorMaker 2), and Apache Pulsar.

## Evidence Integrity Standard (Phase 6 / Workstream F)

In accordance with Kamish's architectural reality and audit principles:
1. **Separation of Baselines and Measured Results:**
   - Manifests with `status: "CONFIGURED_UNEXECUTED"` represent validated benchmark input parameters and comparison baselines.
   - Manifests with `status: "EXECUTED_MEASURED"` represent actual recorded telemetry from a completed, executed workload run.
2. **Durability Parity:**
   - Kamish measurements operate at `AckLevel::LeaderFsync` (local fsync before produce ACK, async cross-region outbox replication).
   - Multi-node in-region quorum durability is not claimed until intra-region cluster consensus is verified.
3. **Artifact Immutability:**
   - Historical evidence files are preserved with unique run IDs (`run-<system>-<timestamp>.json`) and linked to the exact git commit SHA.
