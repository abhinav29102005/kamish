# Hydra Stream — What We Do Differently

*Companion to the Engineering Plan · Draft v0.1 · 31 Oct 2026*

**Research update, 31 Oct 2026:** The prior-art wording below incorporates the
[dated source review](SOURCES_AND_PRIOR_ART.md). These are proposed differences;
the [implementation review](LOOPHOLES_REVIEW.md) records remaining integration
gaps and counterexamples.

## How to read this document

This lists only the parts of Hydra Stream we are proposing as different from existing systems. Everything else (log storage, in-region replication, producer idempotence, and so on) is in the companion baseline document and is deliberately **not** claimed as new.

Two cautions apply to every item below:

- **Comparison basis.** Statements about other systems come from the public documentation and vendor material we reviewed. The review was targeted, not exhaustive, so we word claims as “in the documentation we reviewed” and never as “nobody does this.” Re-check each before submission.
- **Status.** Each item carries a tier from the plan (P0, P1, P2). Only P0 items are committed. If an item is cut, it is removed from the pitch.

## Summary table

| # | Difference | Tier | What it replaces or adds | How we prove it |
| --- | --- | --- | --- | --- |
| 1 | Broker-side residency enforcement at every persistence and send point | P0 | Producer-supplied or gateway-only checks | Canary scan across fault scenarios, including mis-set producer lists |
| 2 | Three destination modes: deny, sealed-only, cleartext | P1 | Binary allow or deny replication | Sealed-copy canary test and key-escrow drill |
| 3 | Transit as an explicit policy dimension | P1 | Routing that ignores relay regions | Route test through a permitted ciphertext-only relay; deny test for others |
| 4 | Per-class survival and degradation rules | P0 | One survival setting for the whole system | Failure matrix, class by class |
| 5 | Fencing built into the broker, gated by policy eligibility | P0 | Operator-run promotion and client reconfiguration | Stale-epoch writer test; partition and clock-skew tests |
| 6 | Evidence package: canaries, decision log, stated limits | P0 / P1 | Configuration screenshots and assertions | Reproducible run script and raw logs |
| 7 | Derive mode with offset lineage | P2 | Transformed topics with no offset relationship | Lineage lookup test |

## 1. Broker-side residency enforcement at every persistence and send point (P0)

**What exists.** Pulsar 5.0.x documents tenant/namespace `allowed-clusters` restrictions enforced independently of a producer's message-level replication list; producers cannot override those restrictions. It also supports topic-level replication targets. Kafka MirrorMaker 2 supports administrator-defined topic-filtered replication flows. The proposed difference must therefore be demonstrated across the full persistence, read, replay, backup and ownership lifecycle, rather than framed as the first server-side replication restriction. See sources S1 and S4 in the [source review](SOURCES_AND_PRIOR_ART.md).

**What we do.** One enforcement library, linked into the ingress, broker, replication sender and receiver, backup or tiered export, consumer fetch, telemetry export and admin tools. A record’s class and the signed policy version are stamped into its envelope at ingress. No component can write to disk or send bytes across a region boundary without calling the same decision function.

**Why it matters.** A gateway check does nothing about a replication worker, a backup job, a retry topic or a replay tool. Server-side checks also remove reliance on every producer getting its list right.

**Proof.** Unique canary records per class; after each fault scenario we scan disks, logs, metrics and packet captures in forbidden regions. A specific test sets a producer’s replication list wrongly and shows the broker still refuses.

**Honest limits.**

- We show absence of leaks only in the locations we scan and the scenarios we run.
- A compromised broker that already holds cleartext can leak it; this design does not defend against that.
- Physical location is taken from infrastructure labels, not proven.

## 2. Three destination modes: deny, sealed-only, cleartext (P1)

**What exists.** Selective replication and client-side end-to-end encryption already exist, including in Pulsar. The proposed addition is a unified class policy selecting deny, ciphertext-only or cleartext for each destination, with explicit key and recovery-location constraints. Ciphertext storage alone is not novel. See S4–S5 in the [source review](SOURCES_AND_PRIOR_ART.md).

**What we do.** Each class has a mode per destination region. A sealed copy is ciphertext only, with keys that stay in the class’s key-residency region. EU regulator guidance lists strong encryption with exporter-held keys as an effective supplementary measure for storage-only transfers such as backup, which is the legal idea behind this mode.

**Why it matters.** It gives a class off-region durability without cleartext leaving its region, where the applicable regime allows it.

**Proof.** Sealed-copy canary test: the destination holds only ciphertext and cannot decrypt. A key-escrow drill: recover a sealed copy after home-region loss using the documented escrow path.

**Honest limits.**

- A sealed copy gives durability, not availability: reading it needs the key.
- If the key is lost with the home region and no escrow exists, the copy is unrecoverable. The plan requires naming the escrow path per class.
- Some regimes may prohibit out-of-border storage even when encrypted, and the guidance relates to specific transfer scenarios. This is a configurable rule, not a compliance claim.
- The key service in the prototype is a software simulation.

## 3. Transit as an explicit policy dimension (P1)

**What exists.** Dossier-style policy models list storage, processing, replication and backup regions. Multipath transport specifications leave path scheduling to the application, and academic wide-area stream processing work uses relay operators for path diversity without policy constraints.

**What we do.** Policy has a transit-allowed list. A relay hop is permitted only if listed and only for ciphertext; the relay sees routing headers, not payload. The route selector first builds the permitted graph, then chooses within it.

**Why it matters.** Rerouting around a slow link through a third region is also a data movement. Treating it as free would break the residency promise at exactly the moment the system is under stress.

**Proof.** Demonstrate rerouting through a permitted relay that holds no cleartext; show a denied reroute through an unlisted region; show the audit entries.

**Honest limits.** Under strict residency the permitted graph is often small, so adaptive routing may have nothing to choose between. We report cases with and without an alternate path, and do not claim routing gains where no alternate exists.

## 4. Per-class survival and degradation rules (P0)

**What exists.** Existing systems expose controls below whole-cluster granularity: Pulsar has topic replication policies, Confluent Cluster Linking manages mirror topics, and Redpanda Shadowing supports individual-topic failover. The proposed difference is the integrated class-specific contract spanning residency, acknowledgment, ownership and backpressure, not granularity alone. See S4, S6 and S7 in the [source review](SOURCES_AND_PRIOR_ART.md).

**What we do.** Each class declares its legal set, ownership mode (movable or pinned), acknowledgment mode, failover candidates and backpressure behavior. A class with no legal alternate region degrades by policy; other classes keep running. Pinned classes need no metadata quorum and keep accepting local writes if the control plane is lost.

**Why it matters.** It makes the trade-off between sovereignty and survivability explicit per class, rather than hiding it behind a single cluster-wide setting, and it is our concrete reading of “survives a regional outage without freezing.”

**Proof.** The failure matrix, run class by class, with automated checkers; a published table of what each class does in each failure.

**Honest limits.**

- A single-region class cannot survive loss of its region; we say so rather than hide it.
- `local-quorum` classes can lose acknowledged-but-unreplicated records on region loss (bounded by replication lag).
- `legal-set-sync(k)` classes avoid that for acknowledged records but pay latency and can block writes when k replicas are unreachable.

## 5. Fencing built into the broker, gated by policy eligibility (P0)

**What exists.** The reviewed Cluster Linking documentation exposes promote/failover plus restore/reverse operations. Redpanda explicitly directs operators to reconfigure clients before resuming writes to avoid split brain. Pulsar replicated subscriptions synchronize the baseline mark-delete cursor, not every individual acknowledgment, so failover can redeliver messages. This targeted review supports testing integrated policy-gated fencing against those documented workflows; it does not establish that every existing product lacks automated failover. See S4, S6 and S7 in the [source review](SOURCES_AND_PRIOR_ART.md).

**What we do.**

- Per-partition `(owner_region, epoch)` held in a small payload-free metadata quorum.
- Time-limited ownership leases; replicas reject appends carrying a lower epoch; an owner that cannot renew its lease stops acknowledging writes and fences itself.
- Promotion requires that policy lists the candidate as eligible, that it holds a replica, that the old lease has expired and that a quorum commits the new epoch.
- A recovered old owner’s superseded log suffix goes to a local quarantine segment, never replicated and never silently deleted.

**Why it matters.** Safety no longer depends on an operator reaching every client in time, and the legal question (may this region take over this class?) is answered by the same policy as everything else.

**Proof.** Stale-writer test (old owner keeps writing after partition: expected stale-epoch commits = 0 in tested scenarios); clock-skew injection; divergent-log recovery test; checker for loss and duplicates.

**Honest limits.**

- Lease safety assumes bounded clock drift between nodes.
- A new owner can be missing records the old owner acknowledged under `local-quorum`; that is the RPO.
- Failover takes about one lease period; we measure and report it rather than promising a number in advance.
- This is a design on our own prototype, not a drop-in for Kafka clusters.

## 6. Evidence package: canaries, decision log, stated limits (P0 / P1)

**What exists.** Residency tooling typically offers configuration, audit logging and compliance snapshots. The tooling we reviewed also notes that audit logs can themselves become regulated data.

**What we do.**

- P0: a canary scanner and invariant checkers (loss, duplicate, order, stale epoch), a one-command reproducible test run, raw logs published with results.
- P1: a region-local, hash-chained decision log that records decisions and policy versions but never payload.
- Every result is accompanied by its scope and a limitations section.

**Why it matters.** A judge or customer can re-run the checks, instead of trusting a diagram.

**Proof.** The run script itself, with seeds and configuration committed.

**Honest limits.** Hash-chaining makes tampering detectable, not impossible. The scan covers observed locations only. We do not claim cryptographic proof of physical location.

## 7. Derive mode with offset lineage (P2)

**What exists.** Offset-preserving mirroring (Cluster Linking, Redpanda Shadowing) works only for exact copies; transformed or sanitized topics are a new log with no formal link back to source offsets.

**What we do.** A derive mode that writes a transformed copy as a new log and records a source-to-derived offset map, so consumers and auditors can trace lineage.

**Honest limits.** Cut first if time is short. If cut, it is removed from all pitch material, not mentioned as a feature.

## What we will not present as new

- Offset-preserving mirror replication (adopted from existing approaches).
- Producer idempotence, per-partition ordering, majority-fsync replication.
- Raft-based metadata, TLS 1.3, standard congestion control.
- Hysteresis and circuit-breaker route stability, which are well-known control techniques; our contribution is only applying them inside the policy-permitted graph.

## Draft novelty statement (adjust to what survives the cut line)

*Hydra Stream is a prototype broker that enforces a signed, versioned residency policy at every point where payload is stored or sent, allows ciphertext-only replicas where policy permits, treats relay hops as policy-controlled data movement, and fences regional failover with epochs gated by the same policy. We evaluate it against Kafka with MirrorMaker 2 and Pulsar geo-replication in an emulated three-region testbed, and report both the cases where it helps and the cases where it cannot.*

## Claims review checklist before submission

1. Does each difference still exist in the final build (tier not cut)?
2. Is each “others do X” statement re-checked against current documentation, with version and date?
3. Does each “we do Y” claim have a test and a run log?
4. Is every guarantee stated with scope and mechanism?
5. Are the limits written next to the claim, not in an appendix?
