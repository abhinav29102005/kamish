# Kamish — configuration and experiment roadmap

**Date:** 2026-10-31 · **Reviewed code:** `4ebfb74375ed2c6ba84e0fdbb4a0d864bb5cc86a`

This document distinguishes existing configuration from proposed interfaces. The TOML example below is a **design proposal**: the current binaries have no `--config` loader for it. Priorities derive from [the loopholes review](LOOPHOLES_REVIEW.md); source IDs refer to [the source register](SOURCES_AND_PRIOR_ART.md).

## 1. What can be configured today?

| Surface | Existing control | Default / limitation |
| --- | --- | --- |
| Broker CLI | `--region`, `--data-dir`, `--port`, `--host`, `--peers` | `region-a`, `./data/broker`, `8080`, `127.0.0.1`, no peers. Debug parsing currently fails on duplicate `-h`. Peers are region names, not a complete region-to-address topology. |
| Agent CLI | `--local-region`, `--target-region`, `--target-addr`, `--data-dir`, `--poll-interval-ms` | Local region `region-a`, data directory `./data/broker`, poll interval 500 ms. One target pair. Constructs its own in-memory broker/outbox; sharing a directory does not share live state with a separate broker process. |
| CLI producer/consumer | Address, topic, partition, class, policy choice, offset/read limit; benchmark count/concurrency | CLI address defaults to port 9092 while broker defaults to 8080. `produce` has conflicting short flags. Policy choices are client inputs, not a deployed authoritative policy service. |
| Signed policy library | `PolicyDocument`, class legal regions, destination modes, `ack_mode`, failover list, transit boolean, version/time fields | Can be constructed and installed via `PolicyCache::update()` in code. No daemon policy-file/trust configuration. Some fields are not integrated into execution. |
| Outbox library | Constructor byte limit and `set_outbox_capacity()` / `set_max_bytes()` | Broker defaults to 5,000,000 estimated bytes per destination. Atomic reservation exists. `Block` and `DegradeLocal` are stored enum values, but the current reservation path rejects at capacity for all modes. |
| Agent library | Queue capacity, retry count/backoff, peer registration, injected `LinkMetrics` | Default 1,024 tasks; 3 retries; 50 ms initial backoff; drain batches of 50 ranges. No separate in-flight byte limit or overall transfer deadline shown. |
| Router source constants | Score weights, hysteresis and dwell | Weights `(0.35, 300, 0.25, 5, 50)`, 15% margin, 5,000 ms dwell. These are source constants, not runtime configuration. |
| Ownership library | Clock-drift margin and lease duration arguments | In-process coordination only; no authoritative distributed lease service wired into produce. |
| Dashboard controls | Delay, partition, traffic rate, adaptive-routing toggle | Browser simulation state; does not configure broker/agent processes. |

**Immediate runnable-system work:** fix flag collisions, unify addresses/region IDs, configure trusted policy input, and decide whether replication runs inside each broker or connects over a real local control interface. Recommended initial implementation: an in-process replication task sharing the broker's state, with durability still derived from the log. Separate-agent deployment can follow a defined IPC/API contract.

## 2. Configuration additions worth implementing

| Area | Proposed settings | Important validation / experiment |
| --- | --- | --- |
| Node identity and topology | Immutable node/region IDs, peer addresses, failure domains, advertised endpoints | Unique IDs; certificate-to-region mapping; avoid mixing aliases such as `region-eu` and `EU_WEST`. Topology labels are configured claims, not proof of geography. |
| Policy loading | Trusted key IDs, signed policy file/service, durable version floor, effective/expiry times, refresh interval, max staleness | Reject invalid signatures, rollback and equal-version different digests; do not accept a producer-selected classification above its permissions. |
| Topic binding | Tenant/topic IDs, class, partitions, stream generation | Bind class server-side; no unregistered auto-creation on arbitrary produce/fetch. Mixed-record filtering needs a different offset contract. |
| Per-class residency | Storage, processing, backup, telemetry, transit and key-residency regions; read/admin roles | Separate permissions by operation; per-record restrictions intersect class permissions; retain effective restrictions through replay. |
| Durability | Ack mode, local replica count/minimum, required remote durable copies, commit deadline | Reject modes not implemented; validate distinct failure domains and promotion eligibility. Timeout means unknown outcome unless rejection is known before commit. |
| Storage | Segment size, index stride, retention time/bytes, disk reserve, checksum/frame limit, fsync/group-commit policy | Retention cannot evict required unreplicated data silently. Crash-test write, sync, rename, directory sync, and recovery transitions. |
| Outbox | Per-stream/per-class disk and memory budgets, host-wide budget, high/low thresholds, backpressure behavior | Persist intent/cursors or guarantee deterministic reconstruction. Reserve serialized/encrypted sizes conservatively. Fairness across classes without reordering a partition. |
| Replication | Max batch records/bytes, in-flight bytes/requests, connection reuse, connect/read/ack deadlines | Byte limits must include staged and retry copies. Resume using receiver durable history, not an unscoped scalar watermark. |
| Retries | Initial/capped delay, jitter, attempt/deadline budget | Retain durable work after transient retries exhaust; avoid one disconnected destination monopolizing workers. |
| Routing | Static/adaptive mode, allowed directed graph, probe/sample windows, EWMA, goodput/backlog thresholds, dwell and breaker cooldown | Hard policy filter on every chosen/current hop. Policy changes invalidate caches immediately. Reuse standard transport congestion control. |
| Ownership | Pinned/movable mode, legal candidates, lease/renew intervals, drift assumption, promotion frontier, quorum endpoints | Persist epoch; reject unknown or uncommitted future epochs; wait for safe prior-lease expiry; recheck before commit/ack. |
| Sealing | AEAD suite, key IDs/versions, nonce strategy, escrow/restore regions, rotation schedule | Expose the mode only after payload encryption and receiver checks exist. No key bytes in ordinary config/audit. Bind stream, offsets, epoch, class, policy digest and key version as authenticated metadata. |
| Transport and API | TLS 1.3, mutual authentication, CA/cert/key references, max frame/payload, authz policy, connection quotas | Authenticate replica/control callers separately from producers; reject plaintext and oversized frames before unbounded allocation. |
| Consumers | Durable group cursor store, checkpoint cadence, replay authorization, epoch-aware resume | Test duplicates after failover against the last usable checkpoint; avoid promising a fixed record bound from a time interval alone. |
| Audit and telemetry | Local sinks, rotation/retention, signed checkpoints, audit failure mode, export allowlist, bounded labels | Encode fields unambiguously; classify topic/class names and labels too. Define whether required audit failure rejects an operation. |
| Testbed | Region/replica counts, workload seed, message size, concurrency, impairment profiles, run ID | Record real measurements and faults independently of dashboard animation. |

## 3. Proposed configuration shape

Illustrative emulation values, **not measured optimal defaults**. This profile requires components that are not implemented today.

```toml
schema_version = 1
node_id = "eu-broker-1"
region_id = "EU_WEST"
listen = "127.0.0.1:9092"
data_dir = "./data/eu-broker-1"

[policy]
signed_file = "./policies/demo.signed.json"
trusted_keys_file = "./identity/policy-public-keys.json"
version_floor_file = "./data/eu-broker-1/policy-floor"
refresh_interval_ms = 5000
max_staleness_ms = 60000

[transport]
tls_version = "1.3"
require_client_certificate = true
ca_file = "./identity/demo-ca.pem"
certificate_file = "./identity/eu-broker-1.pem"
private_key_file = "./identity/eu-broker-1.key"
max_frame_bytes = 2097152
max_payload_bytes = 1048576

[storage]
segment_bytes = 67108864
disk_reserve_bytes = 1073741824
fsync_mode = "before_local_durable_ack"

[replication]
batch_max_records = 64
batch_max_bytes = 1048576
inflight_max_bytes = 8388608
connect_timeout_ms = 2000
ack_timeout_ms = 5000
retry_initial_ms = 50
retry_max_ms = 5000
retry_jitter_fraction = 0.2

[outbox]
memory_limit_bytes_per_stream = 8388608
memory_limit_bytes_total = 134217728
disk_limit_bytes_per_class = 1073741824
on_full = "reject_before_commit"

[routing]
mode = "policy_filtered_static"
sample_window_ms = 1000
ewma_alpha = 0.2
consecutive_windows = 3
hysteresis_fraction = 0.15
minimum_dwell_ms = 5000
breaker_cooldown_ms = 10000

[ownership]
lease_ms = 10000
renew_interval_ms = 2000
assumed_max_clock_drift_ms = 250

[[peers]]
region_id = "US_EAST"
node_id = "us-broker-1"
address = "127.0.0.1:9093"
```

### Configuration semantics

- Runtime limits cannot broaden the signed policy. Keep operator tuning separate from signed authorization.
- Proposed precedence for operational values: defaults → config file → explicitly supported environment overrides → CLI. Report the effective configuration and a redacted configuration hash per run. Fail on unknown keys and conflicting identities.
- Hot reload can apply authorized policy updates and supported rate/threshold changes. Topology, storage-format and consensus-membership changes require their respective protocols. Failed reload retains the last valid configuration, subject to its expiry.
- Validate positive/ranged numeric values, frame overhead against payload size, retry overflow, disk budgets against free space, and total memory across all streams. Lease timing must follow a protocol-derived safety bound; `renew_interval < lease` alone is insufficient.
- Treat format migrations as explicit versions: existing records/policies/audit hashes will need compatibility rules when envelopes or canonicalization change.

### Example class contracts to evaluate

| Class profile | Legal storage / processing | Ack and recovery contract | Expected whole-region-loss behavior |
| --- | --- | --- | --- |
| `eu-local` | EU only / EU only | Local in-region quorum; pinned owner; no remote payload | EU loss makes the class unavailable; no claim of regional survival |
| `eu-us-movable` | EU and US / EU and US | Local quorum + one independent remote durable copy; promote only an eligible replica with acknowledged history | Recovery only within the stated surviving-copy fault model; writes block/reject if required durability cannot be met |
| `eu-sealed-backup` | EU cleartext; US ciphertext-only / EU only | Local quorum plus optional required sealed copy; key/escrow remains within explicitly permitted regions | Sealed copy alone does not make US a valid plaintext-serving failover candidate |

For each profile, define backup and telemetry destinations separately. Do not infer that `US_EAST` being legal for one operation authorizes every operation.

## 4. Size budgets from the workload

For destination stream `s`, with ingress rate `lambda_s` records/s, accounted bytes `B_s` per record, and outage duration `T` seconds:

```text
outage_backlog_bytes ~= lambda_s * B_s * T
catchup_time ~= backlog_bytes / (replication_goodput_bytes_per_s - new_ingress_bytes_per_s)
```

The catch-up expression applies only when replication goodput exceeds continuing ingress. Add explicit space for metadata, journals, indexes, copies, audit and safety headroom. Distinguish physical log bytes from per-destination logical replication obligations when using log-reference queues.

Example assumption: 1,000 records/s × 1 KiB payload × 600 s ≈ 586 MiB **payload only**, before framing and replication copies. A 5 MB queue cannot absorb a ten-minute outage at that rate. Measure actual serialized bytes, peak RSS, and disk amplification before selecting defaults.

## 5. Baseline configuration to compare fairly

### Kafka + MirrorMaker 2 [S1–S3]

Record replication factor (suggested initial local baseline: 3), `min.insync.replicas=2`, `acks=all`, `enable.idempotence=true`, compatible retries/in-flight limits, and `unclean.leader.election.enable=false`. Do not call these identical to fsync-on-every-record: explain the actual disk-flush and failure assumptions for each system.

Configure explicit directional topic allowlists rather than broad default `.*`; record consumer-group filters, checkpoint/offset synchronization, internal-topic placement/replication, and TLS/authentication. Dedicated MM2 source EOS is an optional additional profile, not a universal cross-region transaction claim. Record `exactly.once.source.support`, internal REST coordination/security, and `read_committed` consumption when used. Vary batching/compression transparently rather than suppressing normal optimizations to favor Kamish.

### Pulsar [S4–S5]

Configure tenant/namespace allowed clusters as well as replication targets; include topic overrides and an attempted producer override in the experiment. Record BookKeeper ensemble/write/ack quorum, persistence/flush settings, backlog quota action, retention, deduplication, TLS/authentication, and optional E2EE. Verify exact config names/defaults against the deployed release before generating deployment files.

For consumer failover, record replicated-subscription enablement, snapshot frequency/timeout/cache, active consumer cluster and acknowledgment mode. Snapshot frequency alone does not guarantee a duplicate count or replication interval under backlog. Do not weaken Pulsar to producer-only policy checks.

## 6. Architecture choice

| Choice | Benefit | Main cost / boundary |
| --- | --- | --- |
| Keep the custom Rust broker | Direct control of every storage/send hook and experimental protocol | Must implement and validate local replication, metadata consensus, durable sessions, recovery, TLS and operational APIs |
| Kafka/MM2 substrate plus policy-to-topic control | Mature storage/clients and a realistic baseline | A gateway/connector alone cannot enforce every internal or backup path; exact hooks and deployment controls must be demonstrated |
| Pulsar substrate plus policy-to-namespace/topic control | Existing allowed-cluster enforcement, geo-replication and encryption integration | Extra class modes, recovery/failover policy and cross-path evidence still need implementation and integration analysis |

**Recommendation:** retain the custom code as the experimental implementation while making the substrate decision an explicit organizer/product question. Do not represent a thin gateway as equivalent to complete broker enforcement. Choose one implementation path for the next milestone instead of simultaneously building three systems.

## 7. Prioritized experiments and deliverables

| Phase / experiment | Question / fault | Acceptance evidence |
| --- | --- | --- |
| R0 — contract | Topic-class binding, per-record narrowing, mirror versus derived offsets, ack/ownership modes | Written schemas and incompatible-combination validation; one invariant numbering authority |
| R1 — baseline correctness | Fix L01–L11 as applicable; policy roundtrip, same-sequence concurrency/restart, recovery narrowing, owner mismatch | Research probes become passing regression tests in owning crates; source/receiver history and disk inspection |
| R2 — real boot/restart | Independent processes, configured policy/trust, broker/agent kill between append and queue commit | Restart discovers all streams, resumes from durable per-stream frontier, and produces no gaps/duplicates |
| R3 — bounded recovery | Multiple topics/partitions/classes; one blocked destination; queue/disk saturation | Peak memory/disk, oldest age, per-class reject/block duration, no unrelated cursor trimming, no starvation |
| R4 — policy lifecycle | Revoke while backlog exists, rotate key, rollback after restart, expiry during control-plane loss | Correct version/digest at every send/read; explicit outstanding deletion state; pinned expiry behavior |
| R5 — ownership | Partition control plane and old owner; failover to lagging replica; pause/resume process and clock skew | Cross-region operation history shows zero unauthorized stale-owner commits within the stated assumptions; measured recovery frontier/RPO/RTO |
| R6 — routing | Healthy, bandwidth-limited, jitter/loss, asymmetric and no-alternate topologies | Compare policy-static, policy-adaptive and latency-only controls; show actual chosen network path and policy violations, not just calculated scores |
| R7 — sealing/restore | Ciphertext-only destination, crash during encryption, lost home region, key escrow recovery | Plaintext positive-control scan, nonce uniqueness, metadata tamper rejection, lawful recovery-location enforcement |
| R8 — evidence integrity | Encoded/compressed payload canaries, audit disk failure, hash field changes, valid-suffix truncation | Known-leak detection, audit failure handling, checkpoint verification; precise scanned-location inventory |
| R9 — baselines | Same workload and failure contract on Kafka/MM2 and Pulsar | Reproducible configs, calibration, independent raw histories, latency/lag/resource distributions and configuration-effort comparison |

Run workloads with fixed recorded seeds and multiple repetitions (initial plan: at least five per scenario), plus a sustained run long enough to exceed memory-cache and queue effects. Use a load generator that reports offered, admitted and completed work separately; closed-loop client latency alone hides overload and coordinated omission.

Each run should retain: source commit and binary/dependency versions; redacted config/hash; hardware/OS/filesystem; workload and seed; monotonic-clock durations; fault schedule and measured network conditions; producer ACK/consumer/replica histories; raw metrics; checker results; and explicit exclusions. Readable percentiles without raw observations are not sufficient evidence.

## 8. Work beyond the initial milestone

After correctness and baseline evidence: policy simulation/dry-run impact reports; signed policy rollout status; region-scoped consumer groups; authenticated administrative recovery; tiered backup with deletion provenance; schema compatibility; derived streams with lineage; and adaptive scheduling with cost budgets.

Prioritize these using measured operator or workload needs. Kafka compatibility, multi-cloud deployment and ML-based routing add substantial scope and are not prerequisites to testing the central policy hypothesis.
