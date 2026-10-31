# Sources and prior-art review

**Retrieved:** 2026-10-31. Targeted primary-document review; no comparative deployments or performance measurements were run. Web search returned HTTP 402, so sources were accessed directly. Older Kafka 4.1/Pulsar 4.1 pages were inspected first; the applicable claims below were checked against the displayed Kafka **4.4** and Pulsar **5.0.x** documentation. Exact release artifacts must still be pinned for experiments.

## 1. Source register

| ID | Primary source / edition | Type and date | Finding used / scope |
| --- | --- | --- | --- |
| S1 | [Kafka 4.4 cross-cluster mirroring](https://kafka.apache.org/44/operations/geo-replication-cross-cluster-data-mirroring/) | Apache product docs; page updated 2026-10-22 | Admin-defined directional flows, topic/group filters, ACL and offset synchronization, TLS client settings, and dedicated MM2 exactly-once source support since 3.5.0. This is not a global application-side exactly-once guarantee. |
| S2 | [Kafka 4.4 producer settings](https://kafka.apache.org/44/configuration/producer-configs/) | Apache product docs; 4.4 | Idempotence requires compatible acknowledgments, retries, and in-flight limits. `acks=all` waits for ISR, not a universal per-write physical fsync guarantee. |
| S3 | [Kafka 4.4 topic settings](https://kafka.apache.org/44/configuration/topic-configs/) | Apache product docs; page updated 2026-10-22 | `min.insync.replicas` and unclean leader election are material baseline settings. Record retention, compaction, and replica-placement assumptions too. |
| S4 | [Pulsar 5.0.x geo-replication](https://pulsar.apache.org/docs/5.0.x/administration-geo/) | Apache product docs; 5.0.x | Tenant/namespace allowed-cluster hierarchy, topic replication policies, local persistence with async forwarding, replicated-subscription limits, and replication-read bounds. Do not treat message-level replication lists as the only enforcement mechanism. |
| S5 | [Pulsar 5.0.x end-to-end encryption](https://pulsar.apache.org/docs/5.0.x/security-encryption/) | Apache product docs; 5.0.x | Producers can encrypt payloads and authorized consumers decrypt them; key integration is application controlled. Ciphertext-only broker storage is existing prior art. This is not the same as Kamish's proposed per-destination mode policy. |
| S6 | [Confluent Cloud mirror topics](https://docs.confluent.io/cloud/current/multi-cloud/cluster-linking/mirror-topics-cc.html) | Vendor product docs; rolling `current`, publication date not established | Read-only, byte-for-byte offset-preserving asynchronous mirrors; promote/failover and restore/reverse operations. Product availability and pricing depend on deployment. |
| S7 | [Redpanda Shadowing overview](https://docs.redpanda.com/streaming/current/manage/disaster-recovery/shadowing/overview/) and [failover](https://docs.redpanda.com/streaming/current/manage/disaster-recovery/shadowing/failover/) | Vendor product docs; rolling `current`, publication date not established | Offset-preserving async replication, topic selection and individual-topic failover; enterprise license requirement. Failover docs explicitly require client redirection to avoid split brain. |
| S8 | [NATS mirrors as DR](https://docs.nats.io/learn/backup-recovery/mirrors-and-sources.md) | Project documentation; rolling edition, publication date not established | Cross-site read-only mirrors and async lag are existing mechanisms. Mirrors and point-in-time backups address different failures. Do not equate a sampled zero lag with a future zero-loss guarantee. |
| S9 | [RFC 8785 — JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785.txt) | Independent-submission **Informational** RFC; June 2020 | Stable JSON signing; recursive property ordering. JCS preserves array order, so set-valued arrays need schema-level normalization first. Large integers need an explicit interoperable encoding. |
| S10 | [RFC 5116 — AEAD interface](https://www.rfc-editor.org/rfc/rfc5116.txt) | IETF Standards Track RFC; January 2008 | Authenticated encryption, associated data, and nonce requirements. Use vetted algorithms/libraries and their algorithm-specific limits. |
| S11 | [Raft project overview](https://raft.github.io/) and linked [extended paper](https://raft.github.io/raft.pdf) | Research project; paper introduced in 2014 | Majority-based replicated state-machine agreement. The overview was reviewed; detailed lease/application safety proof and formal model remain follow-up work. A local locked map is not Raft. |
| S12 | [EDPB Recommendations 01/2020](https://www.edpb.europa.eu/our-work-tools/our-documents/recommendations/recommendations-012020-measures-supplement-transfer_en) | Regulator guidance; final version, 18 June 2021 | Official landing page and final-document identity verified. Detailed Annex 2 transfer conditions and applicability to a selected dataset/jurisdiction are **not assessed in this pass**. |

The earlier Confluent `mirror-topics.html` URLs redirected to the documentation homepage. S6 uses the verified `mirror-topics-cc.html` page. The old NATS concepts link redirected to a generic JetStream overview; S8 was discovered through its documentation index. A successful HTTP response alone does not establish that a citation still points to the intended material.

## 2. Capability comparison

“Not established” means the reviewed sources do not establish the capability; it is not a claim that the product cannot provide it through other features or extensions.

| System | Existing configurable capability | Failure/recovery boundary | Remaining Kamish research question |
| --- | --- | --- | --- |
| Kafka + MM2 [S1–S3] | Topic-selected flows, ACLs, TLS, consumer-group/offset sync, local durability controls, optional scoped source EOS | Async inter-cluster copying is distinct from local ISR durability; offset translation and client migration must be considered | Does a single signed policy contract covering reads, replay, backups, and ownership improve outcomes beyond policy-to-topic configuration? |
| Pulsar [S4–S5] | Server-enforced tenant/namespace `allowed-clusters`, topic targets, message-level narrowing, E2EE, replicated subscriptions | Local persistence precedes async geo-replication. Subscription snapshots replicate baseline cursor state; individual acknowledgments and concurrent multi-cluster consumers have limits | Can Kamish justify extra per-class/per-destination modes and lifecycle rules over a strong, correctly configured Pulsar baseline? |
| Confluent Cluster Linking [S6] | Exact offset-preserving mirrors, optional ACL/group-offset sync, topic promotion and recovery/reversal operations | Async mirrors are read-only until transitioned; administrative recovery semantics are documented | Policy-eligible ownership transfer with automatic stale-writer rejection remains a scoped comparison to test, not a proved unique capability |
| Redpanda Shadowing [S7] | Async offset-preserving replication, topic filters, metadata sync, selective topic failover | Enterprise feature; failover requires client coordination described in the runbook | Measure whether integrated fencing reduces operator dependence and stale writes in the specified partition scenario |
| NATS JetStream [S8] | Live cross-site mirrors with lag monitoring, local replication, separate snapshot/restore workflows | A remote mirror can lag and cannot replace point-in-time recovery | Compare simpler mirror/recovery behavior and configuration effort; full jurisdiction-policy coverage is not established by S8 |

## 3. Corrections to the original narrative

1. **“Pulsar depends on producer lists” is incomplete.** S4 says producers cannot override resolved allowed-cluster restrictions. Include that control in the baseline adversarial test.
2. **“Encrypted remote copies are new” is too broad.** E2EE already exists [S5]. The proposed contribution is class-specific destination modes plus key/restore policy and evidence across paths.
3. **“Existing survival controls are whole-cluster only” is too broad.** Topic-level replication and selective failover exist [S4, S6, S7]. Focus on a unified per-class failure contract rather than granularity alone.
4. **“MirrorMaker cannot offer exactly-once” would be false.** S1 documents scoped source EOS. This does not imply exactly-once external side effects, global transactions, or that Kamish should promise them.
5. **Offset preservation, quorum protocols, TLS, hysteresis, and async mirroring remain baseline engineering.** This agrees with the strongest parts of the original dossier.
6. **Signed policies, canaries, and hash chains alone do not establish legal compliance or physical location.** Establish technical scope and dataset-specific policy authority separately.

## 4. Further literature work

- Read the full Raft paper and application-level fencing/lease analyses; model policy epoch, owner epoch, durable frontier, and crash transitions together.
- Evaluate OpenRaft/raft-rs versus an external consensus service for Rust integration. Review exact versions, storage adapters, snapshots, membership change, maintenance and licenses before choosing.
- Verify Kafka MM2 checkpoint behavior and Pulsar allowed-cluster enforcement in the exact deployed binaries, including adversarial client lists and policy updates during backlog recovery.
- Review transport RFCs 9000, 8684 and 8446 referenced by the dossier if transport changes become necessary. A new congestion-control algorithm is not needed for the initial experiment.
- Choose jurisdictions/data classes before interpreting regulator guidance. Account for storage, processing, keys, operator access, backups and metadata separately.
- Review stronger prior art on policy-as-code lifecycle enforcement and data lineage before asserting publication-level novelty. This pass supports a narrower engineering hypothesis, not an exhaustive novelty search.
