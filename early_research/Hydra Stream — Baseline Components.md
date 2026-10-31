# Hydra Stream — Baseline Components (Table Stakes)

*Companion to the Engineering Plan · Draft v0.1 · 31 Oct 2026*

## Purpose

These are the parts of Hydra Stream that are standard engineering. We build or reuse them because the differentiating features sit on top of them, not because they are new. Nothing here is presented as a contribution. The aim is to get each one correct, tested and boring.

For each component this document gives the approach, whether we reuse a library or write it ourselves, the invariant it must keep, and how we know it works.

## Build or reuse at a glance

| # | Component | Build or reuse | Tier |
| --- | --- | --- | --- |
| 1 | Segmented append-only log | Build (small) | P0 |
| 2 | In-region replication | Build on a Raft or quorum-fsync design | P0 |
| 3 | Producer and consumer API | Build (gRPC) | P0 |
| 4 | Idempotent producers and offsets | Build | P0 |
| 5 | Mirror-mode replication between regions | Build | P0 |
| 6 | Durable outbox and resumable transfer | Build | P0 |
| 7 | Metadata quorum | Reuse a Raft library | P0 |
| 8 | Transport security and identity | Reuse TLS 1.3 and standard certificates | P0 |
| 9 | Link measurement | Build (small) | P1 |
| 10 | Region emulation and fault injection | Reuse Linux tools and containers | P0 |
| 11 | Checkers and metrics | Build checkers, reuse metrics libraries | P0 |
| 12 | Baseline systems (Kafka with MirrorMaker 2, Pulsar) | Deploy as documented | P0 |
| 13 | Policy signing and verification | Reuse standard signature primitives | P0 |

Build decisions are provisional. If organizers allow building on Kafka or Pulsar, components 1 to 4 could be replaced by an existing broker, at the cost of weaker control over the storage and send path that the enforcement library depends on.

## 1. Segmented append-only log

**Approach.** Partitions are sequences of fixed-size segment files with an index from offset to file position. Appends are sequential. A record batch carries the partition epoch, producer identity and sequence, class, policy version and a checksum.

**Invariants.** Offsets are dense and monotonic within a partition. A batch is acknowledged only after it is durable according to its acknowledgment mode. A corrupt tail is detected by checksum and truncated on recovery.

**Verification.** Crash-recovery tests that kill the process at random points during writes; checksum corruption tests; ordering checks over long runs.

**Not claimed.** Throughput or latency leadership. We measure and report our own numbers on our own hardware.

## 2. In-region replication

**Approach.** Each partition has three replicas across brokers in a region. The leader acknowledges after a majority has fsynced. The leader is elected through a standard quorum protocol inside the region. A follower that falls behind is removed from the acknowledgment set until it catches up.

**Invariants.** An acknowledged record is on a majority of in-region replicas. A new leader holds all acknowledged records.

**Verification.** Broker-kill tests during sustained writes; the loss, duplicate and ordering checker confirms no acknowledged record is missing.

**Note.** Independent testing of Kafka-compatible systems has found safety bugs in the past, so we test our own implementation rather than assume correctness from the design.

## 3. Producer and consumer API

**Approach.** A small gRPC and Protobuf API: create topic with class, produce batch, fetch from offset, commit and read consumer offsets, list ownership. Authentication is mutual TLS with per-client identity.

**Invariants.** A fetch returns records in offset order and never beyond the committed high-water mark. Consumers in a region may fetch only classes that policy allows for that region (enforcement hooks are covered in the differentiators document).

**Verification.** API contract tests and a reference client used by the load generator.

**Stretch, not committed.** A Kafka wire-protocol subset would let standard clients and existing test harnesses connect. It is P2 and cut first.

## 4. Idempotent producers and consumer offsets

**Approach.** Each producer session has an id and an increasing sequence per partition. The broker rejects duplicates and out-of-order sequences. The dedupe state is recoverable from the log itself so a new leader or a promoted region can rebuild it. Consumers commit offsets; offsets are checkpointed together with the partition epoch.

**Invariants.** Retries by the same session do not create duplicate appends. After failover, consumers may see duplicates bounded by the checkpoint interval, never gaps for acknowledged data that exists in the new owner’s log.

**Verification.** Retry-storm tests with injected timeouts; failover tests that count duplicates and verify the bound.

**Not claimed.** Exactly-once delivery or transactions. Kafka’s transaction protocol has documented anomalies in independent analysis, and we do not offer an equivalent.

## 5. Mirror-mode replication between regions

**Approach.** For a class whose policy allows a destination in cleartext, the sender ships record batches in offset order and the destination appends them under the same offsets. Preserving offsets is the approach used by Confluent Cluster Linking and Redpanda Shadowing, and it avoids the lossy offset translation of older mirroring tools.

**Invariants.** The destination log is a prefix of the source log at all times. Offsets match exactly.

**Verification.** Compare checksums of source and destination ranges after each scenario; prefix check during replication.

**Attribution.** We adopt an existing idea. Whatever sits around it (policy gate, epochs, modes) is covered in the differentiators document.

## 6. Durable outbox and resumable transfer

**Approach.** Replication work is queued on disk per destination and partition, bounded by a size limit per class. A transfer is identified by partition, epoch and offset range. The receiver acknowledges a durable watermark; the sender resumes from that watermark after any failure.

**Invariants.** Order is preserved per partition. A retry of an already-applied range is a no-op. Memory use is bounded; at the disk bound the class’s backpressure rule applies.

**Verification.** Pull the network for a configured duration and reconnect; kill sender and receiver mid-transfer; confirm no gaps, duplicates or memory growth beyond the bound.

## 7. Metadata quorum

**Approach.** A three-voter Raft group, one voter per region, stores ownership records, policy versions and membership. We use an existing Raft library rather than write consensus ourselves, and pin the version after checking its license and maintenance status.

**Invariants.** Committed metadata is never lost while a majority survives. Metadata contains no payload and is off the write path.

**Verification.** Leader-kill, partition and quorum-loss tests; confirm no committed ownership change is lost.

**Limit.** Cross-region consensus adds latency to ownership changes, which are infrequent, not to ordinary writes.

## 8. Transport security and identity

**Approach.** TLS 1.3 with mutual authentication between all components. For the prototype, certificates are generated per region from a test certificate authority. Production would use workload identity; that is out of scope.

**Invariants.** No plaintext inter-region connections. Every component presents an identity that policy can refer to.

**Verification.** Scan test traffic for plaintext; reject-unknown-client tests.

**Reminder.** Encryption in transit does not by itself establish lawful residency. It is a baseline control, not a compliance argument.

## 9. Link measurement

**Approach.** Each replication agent records per-destination smoothed round-trip time and variance, loss or retransmission rate, goodput over a window and oldest unsent record age. Values are exported to the route selector. Measuring uses the live replication connections plus lightweight probes.

**Invariants.** Measurements are windowed and smoothed; a single sample cannot trigger a route change.

**Verification.** Replay recorded traces; compare estimates with the emulator’s configured conditions.

**Limit.** Round-trip time alone does not measure usable capacity, so goodput and backlog are included.

## 10. Region emulation and fault injection

**Approach.** Three logical regions as containers or network namespaces. Delay, jitter, loss, bandwidth limits and partitions are applied with standard Linux traffic control. Clock skew is injected for lease tests. Scenarios are scripted and parameterized so every run is repeatable.

**Invariants.** Each scenario has a definition, a start and end, and a recorded configuration.

**Verification.** Calibrate the emulator by measuring achieved delay and bandwidth against what was configured, and publish the calibration.

**Limit.** A single-host emulation approximates real wide-area behavior. We state hardware and treat ratios between configurations as more meaningful than absolute numbers.

## 11. Checkers and metrics

**Checkers we write.**

- Loss and duplicate checker over producer acknowledgments and consumer reads.
- Per-partition ordering checker.
- Canary scanner for forbidden regions (its logic is part of the differentiators document; its tooling lives here).
- Epoch auditor that scans logs for appends with a stale epoch.

**Metrics we collect.** Publish throughput, end-to-end latency percentiles, replication lag in time and offsets, outbox depth and oldest age, measured RPO and RTO per scenario, route switches, rejected writes, resource use. Implemented with common metrics libraries, exported only under the telemetry policy.

**Method.** Fixed seeds, several repetitions per scenario, medians and spread reported, raw logs kept.

## 12. Baseline systems

**Approach.** Deploy Kafka with MirrorMaker 2, and Pulsar with geo-replication, following their documentation, on the same emulated topology, with matching message size, partitions, retention and durability settings.

**Fairness rules.** Record every setting. List differences that cannot be normalized. Do not tune baselines less than our own system. Where a baseline lacks a feature (for example, a per-record server-side residency check), compare on measurable outcomes rather than listing the absence as a defect.

**Verification.** Each baseline passes its own smoke test and a replication-lag measurement before any comparison run.

## 13. Policy signing and verification

**Approach.** Policies are signed offline by a policy authority using a standard signature algorithm and verified by every region before use. Version numbers increase monotonically; regions reject older versions.

**Invariants.** An unsigned, invalid or rolled-back policy is never applied. A policy past its staleness limit puts the restricted class into fail-closed mode.

**Verification.** Tests with tampered, replayed and expired policy objects.

## Shared engineering practices

- Pin dependency versions; record licenses.
- Run the invariant checkers in continuous integration on a small scenario set.
- Write each component’s failure behavior down before testing it.
- Keep a results log: every reported number links to a run, configuration and date.

## Exit criteria for the baseline

1. A region ingests, replicates in mirror mode and serves reads under healthy conditions with all checkers green.
2. Broker loss, network pause and process crash scenarios pass the loss, duplicate and ordering checks.
3. The metadata quorum survives single-voter loss without losing committed state.
4. Baselines run on the same testbed and produce comparable lag measurements.
5. The scripted scenario runner reproduces a run from its recorded configuration.

Only after these hold should effort shift to the differentiating features.
