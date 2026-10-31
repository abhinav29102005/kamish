# Hydra Stream — What We Do Differently

*Companion to the Engineering Plan · Draft v0.1 · 31 Oct 2026*

**Research update, 31 Oct 2026:** The prior-art wording below incorporates the
[dated source review](SOURCES_AND_PRIOR_ART.md). These are proposed differences;
the [implementation review](LOOPHOLES_REVIEW.md) records remaining integration
gaps and counterexamples.

**Phase 0 update, 1 November 2026:** The candidate contribution is one signed
class policy applied consistently across admission, storage, replication, replay,
recovery and ownership change, with scope-specific evidence. Pulsar already
enforces allowed clusters server-side and ciphertext storage is prior art.
[Contract choices](../docs/CONTRACT.md) await approval; this document describes
proposals, not a completed distributed implementation. Sealing/relay/derive are
outside the currently authorized phases.

## How to read this document

This lists only the parts of Hydra Stream we are proposing as different from existing systems. Everything else (log storage, in-region replication, producer idempotence, and so on) is in the companion baseline document and is deliberately **not** claimed as new.

Two cautions apply to every item below:

- **Comparison basis.** Statements about other systems come from the public documentation and vendor material we reviewed. The review was targeted, not exhaustive, so we word claims as “in the documentation we reviewed” and never as “nobody does this.” Re-check each before submission.
- **Status.** Each item carries a tier from the plan (P0, P1, P2). Only P0 items are committed. If an item is cut, it is removed from the pitch.

## Summary table

| # | Difference | Tier | What it replaces or adds | How we prove it |
| --- | --- | --- | --- | --- |
| 1 | Same signed class policy across persistence and lifecycle boundaries | P0 | Integration to compare with existing server-side restrictions | Full-path canary/history tests, including incorrect producer lists |
| 2 | Per-destination mode plus explicit key/recovery policy | P1 | Proposed policy integration over existing ciphertext storage | Secure sealed-copy/restore tests; runtime disabled and outside current work |
| 3 | Transit as an explicit policy dimension | P1 | Routing that ignores relay regions | Route test through a permitted ciphertext-only relay; deny test for others |
| 4 | Per-class survival and degradation rules | P0 | Unified contract spanning existing topic-level replication/failover controls | Failure matrix, class by class |
| 5 | Fencing built into the broker, gated by policy eligibility | P0 | Operator-run promotion and client reconfiguration | Stale-epoch writer test; partition and clock-skew tests |
| 6 | Evidence package: canaries, decision log, stated limits | P0 / P1 | Configuration screenshots and assertions | Reproducible run script and raw logs |
| 7 | Derive mode with offset lineage | P2 | Transformed topics with no offset relationship | Lineage lookup test |

## 1. Broker-side residency enforcement at every persistence and send point (P0)

**What exists.** Pulsar 5.0.x documents tenant/namespace `allowed-clusters` restrictions enforced independently of a producer's message-level replication list; producers cannot override those restrictions. It also supports topic-level replication targets. Kafka MirrorMaker 2 supports administrator-defined topic-filtered replication flows. The proposed difference must therefore be demonstrated across the full persistence, read, replay, backup and ownership lifecycle, rather than framed as the first server-side replication restriction. See sources S1 and S4 in the [source review](SOURCES_AND_PRIOR_ART.md).

**Proposed integration.** One enforcement library across ingress, broker, sender,
receiver, fetch, replay, recovery, ownership, backup and telemetry. A source record
retains its class and policy version. Only selected paths currently exist; the
full integration must be demonstrated rather than inferred from action enums.

**Why it matters.** A gateway check does nothing about a replication worker, a backup job, a retry topic or a replay tool. Server-side checks also remove reliance on every producer getting its list right.

**Proof.** Unique canary records per class; after each fault scenario we scan disks, logs, metrics and packet captures in forbidden regions. A specific test sets a producer’s replication list wrongly and shows the broker still refuses.

**Honest limits.**

- We show absence of leaks only in the locations we scan and the scenarios we run.
- A compromised broker that already holds cleartext can leak it; this design does not defend against that.
- Physical location is taken from infrastructure labels, not proven.

## 2. Three destination modes: deny, sealed-only, cleartext (P1)

**What exists.** Selective replication and client-side end-to-end encryption already exist, including in Pulsar. The proposed addition is a unified class policy selecting deny, ciphertext-only or cleartext for each destination, with explicit key and recovery-location constraints. Ciphertext storage alone is not novel. See S4–S5 in the [source review](SOURCES_AND_PRIOR_ART.md).

**Proposed integration.** A class selects destination modes and explicit key/
recovery constraints. Runtime sealed transfer currently rejects. Applying any
regulator guidance to a dataset/jurisdiction needs a separate assessment; the
dated source review verified document identity, not those application conditions.

**Why it matters.** It gives a class off-region durability without cleartext leaving its region, where the applicable regime allows it.

**Proof.** Sealed-copy canary test: the destination holds only ciphertext and cannot decrypt. A key-escrow drill: recover a sealed copy after home-region loss using the documented escrow path.

**Honest limits.**

- A sealed copy gives durability, not availability: reading it needs the key.
- If the key is lost with the home region and no escrow exists, the copy is unrecoverable. The plan requires naming the escrow path per class.
- Some regimes may prohibit out-of-border storage even when encrypted, and the guidance relates to specific transfer scenarios. This is a configurable rule, not a compliance claim.
- The key service in the prototype is a software simulation.

## 3. Transit as an explicit policy dimension (P1)

**What exists.** Dossier-style policy models list storage, processing, replication and backup regions. Multipath transport specifications leave path scheduling to the application, and academic wide-area stream processing work uses relay operators for path diversity without policy constraints.

**Proposed integration.** An explicit transit list constrains ciphertext relay
hops before route scoring. Current planner checks do not implement relay
networking or establish encrypted representation. Relay remains outside current work.

**Why it matters.** Rerouting around a slow link through a third region is also a data movement. Treating it as free would break the residency promise at exactly the moment the system is under stress.

**Proof.** Demonstrate rerouting through a permitted relay that holds no cleartext; show a denied reroute through an unlisted region; show the audit entries.

**Honest limits.** Under strict residency the permitted graph is often small, so adaptive routing may have nothing to choose between. We report cases with and without an alternate path, and do not claim routing gains where no alternate exists.

## 4. Per-class survival and degradation rules (P0)

**What exists.** Existing systems expose controls below whole-cluster granularity: Pulsar has topic replication policies, Confluent Cluster Linking manages mirror topics, and Redpanda Shadowing supports individual-topic failover. The proposed difference is the integrated class-specific contract spanning residency, acknowledgment, ownership and backpressure, not granularity alone. See S4, S6 and S7 in the [source review](SOURCES_AND_PRIOR_ART.md).

**Proposed integration.** Each class declares permitted regions, ownership,
acknowledgment, failover and backpressure behavior. Distinct class budgets and
failure evidence must demonstrate isolation. Pinned continuity is a design
requirement, not current behavior; the strict-authority option in the pending
Phase 0 contract would leave NFR-01 unresolved.

**Why it matters.** It makes the trade-off between sovereignty and survivability explicit per class, rather than hiding it behind a single cluster-wide setting, and it is our concrete reading of “survives a regional outage without freezing.”

**Proof.** The failure matrix, run class by class, with automated checkers; a published table of what each class does in each failure.

**Honest limits.**

- A single-region class cannot survive loss of its region; we say so rather than hide it.
- `local-quorum` classes can lose acknowledged-but-unreplicated records on region loss (bounded by replication lag).
- Synchronous regional durability requires explicit distinct-region counting,
  surviving-copy assumptions and verified promotion history; the meaning of K
  and its blocking behavior await Phase 0 approval.

## 5. Fencing built into the broker, gated by policy eligibility (P0)

**What exists.** The reviewed Cluster Linking documentation exposes promote/failover plus restore/reverse operations. Redpanda explicitly directs operators to reconfigure clients before resuming writes to avoid split brain. Pulsar replicated subscriptions synchronize the baseline mark-delete cursor, not every individual acknowledgment, so failover can redeliver messages. This targeted review supports testing integrated policy-gated fencing against those documented workflows; it does not establish that every existing product lacks automated failover. See S4, S6 and S7 in the [source review](SOURCES_AND_PRIOR_ART.md).

**Proposed integration; not implemented as a distributed protocol.**

- Per-partition `(owner_region, epoch)` held in a small payload-free metadata quorum.
- Time-limited ownership leases; replicas reject appends carrying a lower epoch; an owner that cannot renew its lease stops acknowledging writes and fences itself.
- Promotion requires current policy eligibility, a verified committed replica
  frontier, exclusion of the prior owner and a quorum-committed grant. A caller's
  `holds_replica` boolean is insufficient.
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

**Proposed evidence package.**

- P0: a canary scanner and invariant checkers (loss, duplicate, order, stale epoch), a one-command reproducible test run, raw logs published with results.
- P1: a region-local, hash-chained decision log that records decisions and policy versions but never payload.
- Every result is accompanied by its scope and a limitations section.

**Why it matters.** A judge or customer can re-run the checks, instead of trusting a diagram.

**Proof.** The run script itself, with seeds and configuration committed.

**Honest limits.** An internal chain detects covered edits but cannot detect a
removed valid suffix without an independent anchor. Periodic anchors cover only
their committed prefix; later entries remain unwitnessed. Scans cover observed
locations only and do not prove physical location.

## 7. Derive mode with offset lineage (P2)

**What exists.** Offset-preserving mirroring (Cluster Linking, Redpanda Shadowing) works only for exact copies; transformed or sanitized topics are a new log with no formal link back to source offsets.

**Proposed, outside current work.** A transformed log with an explicit source-to-
derived offset map; this is not an implemented API.

**Honest limits.** Cut first if time is short. If cut, it is removed from all pitch material, not mentioned as a feature.

## What we will not present as new

- Offset-preserving mirror replication (adopted from existing approaches).
- Producer idempotence, per-partition ordering, majority-fsync replication.
- Raft-based metadata, TLS 1.3, standard congestion control.
- Hysteresis and circuit-breaker route stability, which are well-known control techniques; our contribution is only applying them inside the policy-permitted graph.

## Draft novelty statement (adjust to what survives the cut line)

*Kamish investigates one recoverable signed class-policy contract across
admission, storage, replication, replay, recovery and ownership change. Selected
local paths have tests; the complete lifecycle and equivalently configured,
version-pinned Kafka/MirrorMaker 2 and Pulsar comparisons remain to be built and
measured. Results must identify the mechanism, fault model and observed scope.*

## Claims review checklist before submission

1. Does each difference still exist in the final build (tier not cut)?
2. Is each “others do X” statement re-checked against current documentation, with version and date?
3. Does each “we do Y” claim have a test and a run log?
4. Is every guarantee stated with scope and mechanism?
5. Are the limits written next to the claim, not in an appendix?
