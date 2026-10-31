# Kamish — loopholes and design review

**Date:** 2026-10-31 · **Revision:** `4ebfb74375ed2c6ba84e0fdbb4a0d864bb5cc86a`

Inputs: all four original `early_research/` documents, `project.md`, core Rust modules, frontend state, and milestone 2/3 reports. External sources are indexed in [the source register](SOURCES_AND_PRIOR_ART.md). Reproductions are in [PROBE_RESULTS.md](PROBE_RESULTS.md).

## 1. Findings that change the plan

- **One previous defect is fixed:** atomic reservation now rejects a write before persistence when a required outbox lacks capacity. The previous ignored-enqueue finding must not be reported as current.
- **Recovery is partially implemented:** `repopulate_outbox_from_log()` and an explicit handshake can reconstruct work. Automatic daemon startup discovery/recovery is missing, and reconstructed work loses request-level restrictions.
- **Library tests do not establish distributed guarantees:** 61 application tests pass, while 10 of 11 independent contract probes fail.
- **The broad novelty claim needs narrowing:** Pulsar has server-enforced allowed-cluster restrictions and end-to-end encryption; Kafka MM2 has configurable replication flows and scoped exactly-once source support. Kamish needs to demonstrate the additional integrated policy/recovery/ownership behavior.

Priority here means implementation order: **P0** blocks the promised correctness or a runnable system; **P1** is needed before the corresponding optional feature is enabled. It does not change the original project's feature tiers.

## 2. Reproduced loopholes

| ID | Priority | Counterexample and impact | Source at reviewed revision | Required correction |
| --- | --- | --- | --- | --- |
| L01 | P0 if sealed mode exposed | A range marked sealed retains the original payload; the receiver persists it unchanged. | `cmd/kamish-broker/src/lib.rs:413–424`, `:459–499`; agent forwards the range payload | Encrypt before egress; enforce a validated sealed envelope at receiver; reject unsupported modes. |
| L02 | P0 | A caller requests an allowed class while fetching a partition containing a class whose fetch is denied. Restricted records are returned. | `cmd/kamish-broker/src/lib.rs:437–455` | Resolve class from authenticated topic metadata and verify each returned record where mixed classes are supported. |
| L03 | P0 | Acknowledging partition 0 at watermark 1 removes partition 1's unreplicated offset 0. | `crates/kamish-outbox/src/lib.rs:62–67`, `:168–198`, `:227–235` | Scope ranges, acknowledgments, membership checks, and durable cursors to stream identity including destination, topic ID, partition, and epoch/generation. |
| L04 | P0 | Eight concurrent retries of the same producer sequence append eight records. | `cmd/kamish-broker/src/lib.rs:319–410` | Make session validation, append, and session advancement one serialized per-partition operation. Reject reuse of an identity/sequence with different content. |
| L05 | P0 | Retrying sequence 0 after restart appends at offset 1 instead of returning offset 0. | `crates/kamish-log/src/lib.rs:45–53`; broker `get_or_create_session()` | Persist producer ID, producer epoch, sequence, and result; rebuild or snapshot dedupe state before admitting requests. |
| L06 | P0 | Reopening a broker recovers the log but leaves pending outboxes empty without an explicit recovery call. | `cmd/kamish-broker/src/lib.rs:224–242`, `:563–612`; `cmd/kamish-agent/src/main.rs` | Discover durable streams at startup; recover continuously in bounded pages after a watermark handshake. Handle replay errors rather than discarding them. |
| L07 | P0 | JSON serialize/deserialize changes a valid policy's verification result although the policy fields are equal. | `crates/kamish-policy/src/lib.rs:89–105` | Define canonical signed bytes, normalize set-valued fields, and test cross-process roundtrips. |
| L08 | P0 | `StrictLocal` blocks an initial egress, but recovery re-enqueues that record using only its permissive class. | `cmd/kamish-broker/src/lib.rs:261–276`, `:582–607`; `RecordEnvelope` | Persist effective restrictions and origin; run the same decision contract on live, recovery, backup, and replay paths. |
| L09 | P0 | Broker A acknowledges an epoch-1 write even when its own metadata says B owns the partition at epoch 7. | `cmd/kamish-broker/src/lib.rs:316`; `crates/kamish-own/src/lib.rs:199–213` | Integrate owner/epoch/lease validation into append and acknowledge boundaries, backed by authoritative metadata. |
| L10 | P1 audit integrity | `class="ab", source="c"` and `class="a", source="bc"` produce the same audit hash input. This is ambiguous encoding, not a SHA-256 collision. | `crates/kamish-audit/src/lib.rs:118–129` | Hash length-delimited or canonical structured fields with a format version/domain separator. |
| L11 | P0 operability | Debug CLI parsing panics: broker `host` collides with help on `-h`; `ctl produce` has multiple `-p` options. | `cmd/kamish-broker/src/main.rs:23`; `cmd/kamish-ctl/src/main.rs:49–58` | Assign unique short options or use long-only flags; test every subcommand's help. |

L06 is specifically a **startup integration** failure. The in-tree restart test explicitly calls the reconstruction helper; that test and this finding are compatible. L01's probe uses the same payload forwarding as the agent through the receiver API and checks the recovered disk record; it is not a packet-capture experiment.

## 3. Further source-inspected gaps

These findings have code evidence but were not all exercised by the research harness.

### 3.1 Durability and ownership

- **Local fsync is not an in-region quorum.** `PartitionLog::append()` syncs one local file, then advances HWM. `AckMode` is not applied by `produce()`. `LocalMajority`, `RegionMajority`, and `QuorumAll` do not implement the design's `local-quorum` and `legal-set-sync(k)` contracts merely by existing as enum variants.
- **Metadata is an in-process map.** `MetadataStore` uses `Arc<RwLock<...>>`; CAS across threads is not consensus across processes. No metadata-voter daemon is in the workspace. Independently created brokers have independent authority.
- **Fencing is not recovered from disk.** `PartitionLog::open()` initializes `current_epoch` from the caller rather than recovering an authoritative fence. Unknown ownership and a future arbitrary epoch require explicit rejection semantics.
- **Candidate safety is a boolean argument.** `try_promote(..., holds_replica: bool, ...)` does not establish a durable commit frontier. A candidate needs verified epoch/history and acknowledged-prefix eligibility, not just possession of some replica.
- **Quarantine is incomplete.** `reconcile_divergent_suffix()` copies a supplied vector to quarantine; it does not find a shared prefix, truncate the active log, or reconcile sessions/consumer cursors. Quarantine rewrites a JSON file and flushes without a durable atomic replace protocol.
- **Concurrent lazy log open is risky.** `get_or_create_log()` opens/replays a disk path before the winning map insertion. A losing opener can run recovery against a file the winning instance is writing. Require one initializer and one writer owner per physical partition path; add a race/fault-injection test.

### 3.2 Policy and identity

- **Policy authority is incomplete at startup.** Broker and agent mains build empty trusted-issuer caches; the wire request enum has no policy-publish operation. Even after fixing argument parsing, normal daemon clients encounter `no_policy_loaded`.
- **Two policy engines disagree about the source of truth.** Signed class rules coexist with optional request-supplied `ResidencyPolicy`; absent overrides are guessed from class-name substrings. The signed topic binding should impose the maximum permissions; producer overrides may only narrow them.
- **Missing policy dimensions:** distinct storage/processing/backup regions, roles, per-class staleness, retention, key residence, pinned ownership, and per-region transit lists are in the design but not fully represented/enforced in `ClassRule`. `transit_rules` exists in the policy document but relay enforcement checks only a boolean and ciphertext flag.
- **Rollback protection is volatile.** Highest-seen version disappears with `PolicyCache`; equal-version replacement is accepted. Persist a policy-ID/version/digest floor and define same-version identical refresh versus equivocation. Signing-key rotation and emergency revocation need a protocol.
- **Transport is plaintext framed TCP.** There is no mTLS identity binding between a connection and its claimed source/consumer region. TLS endpoints and workload identities are separate from physical location evidence.
- **Topic names become filesystem paths.** `data_dir.join(format!("{}-{}", topic, partition))` has no topic-ID/path validation. Add immutable topic IDs, name validation, and containment checks before opening storage.

### 3.3 Sealed copies and routing

- **Replace the encryption construction.** `kamish-seal` repeats key-derived XOR bytes without a nonce and labels `SHA256(key || ciphertext)` as HMAC. It is neither HMAC nor a vetted AEAD construction. Known plaintext exposes reusable keystream. Use a standard AEAD, nonce uniqueness across restart, authenticated envelope metadata, key versions, and rotation/escrow tests. See S10.
- **Route caching can violate the hard filter.** `RouteSelector` caches by destination only and can return the previous candidate while dwell/hysteresis prevents a switch, without establishing that it remains permitted for the current class/policy. Cache by policy context and revalidate retained paths; revocation must bypass dwell timers.
- **The cached cost is stale.** The current path retains its old metrics; path equality includes metric values. Define stable path identity separately from measurements, compare fresh costs, and reject non-finite inputs.
- **Adaptive routing is not integrated.** Agents supply one direct candidate and send directly to the registered peer. Relay transport, measured goodput, consecutive-window decisions, and circuit-breaker transitions are not demonstrated. The router supplies `with_ciphertext(true)` itself rather than deriving it from an encrypted envelope.

### 3.4 Resource bounds and evidence

- **A bounded outbox is not bounded broker memory.** Complete partition logs and audit entries are held in vectors; TCP `lines()` has no configured frame limit; requests clone payloads; task count is not a byte budget. `payload.len() + 64` omits variable metadata, JSON expansion, and additional copies.
- **Reservations are admission control, not crash transactions.** A process can die between durable append and in-memory range commit. Durable intent or deterministic replay is needed. Public `OutboxReservation::commit()` can also increase accounting above the originally reserved size without another bound check; encrypted-size expansion needs a defined reservation contract.
- **Retry scheduling has no overall deadline/jitter shown.** One worker can spend its retry budget on an unreachable peer while other work waits. Use per-stream ordering plus fair, bounded scheduling across independent streams.
- **Audit success is not enforced.** `Enforcer::record_decision()` ignores audit errors; daemon startup can fall back to in-memory audit on open failure. Decide and expose the failure policy. A hash chain alone cannot detect removal of a valid suffix without a trusted checkpoint.
- **The canary scanner searches raw byte substrings.** JSON encodes payload bytes as numeric arrays; compression, encodings, packet fragmentation, and TLS defeat naive substring scans. Decode artifacts, instrument plaintext boundaries where permitted, and test scanner-positive controls. The epoch checker checks one sequence's monotonicity, not cross-region ownership histories.
- **Dashboard telemetry is simulated.** `frontend/src/context/KamishContext.tsx` generates values locally; it cannot establish broker performance, failover, or audit integrity.

## 4. Design loopholes to resolve before adding features

### A. Per-record filtering conflicts with exact mirror prefixes

Example: source offsets 0 and 2 may go to B; offset 1 must stay in A. Sending 0 then 2 leaves a hole, while renumbering 2 breaks exact mirroring. A later policy revocation creates the same problem even if the original topic was homogeneous.

**Recommended P0:** bind each topic/partition to one class and replication contract; changes requiring a different history create a new stream generation. Explicitly define already-admitted data handling on policy change. For mixed classes later, choose a filtered/derived log with lineage, or policy-approved redacted position markers. Markers themselves leak metadata and are not automatically permissible.

### B. A copy count alone does not prove zero loss

`legal-set-sync(k)` must specify independent failure domains, which replicas can lead, how the acknowledged frontier is discovered, and what promotion does when only an incomplete candidate survives. A sealed replica without an accessible lawful recovery key cannot immediately serve plaintext. Report zero loss only for a named fault set with surviving durable acknowledged copies and a recovery protocol.

### C. Pinned availability still depends on policy freshness

A pinned class can avoid global ownership leases, but it cannot accept restricted writes forever after its authorization expires and also promise fail-closed freshness. Specify the cached-policy validity window; separate ownership quorum loss from policy-expiry behavior.

### D. Revocation and deletion are different protocols

Stopping future sends does not remove existing remote copies, backups, quarantine, or keys. Define `effective_at`, handling of in-flight transfers, current-policy checks on reads, purge acknowledgments, and restore-time tombstone checks. A disconnected region cannot acknowledge immediate deletion. Retention, legal hold, and replication-required retention need explicit precedence.

### E. Consumer duplicate bounds require durable progress

“Bounded by checkpoint interval” is a time statement, not a fixed record count. At variable throughput, the count is the acknowledged work after the last usable checkpoint. Current network APIs lack durable consumer offset commits; include epoch/history in the cursor and document rewind behavior. Consumer external side effects remain outside broker idempotence.

### F. Logical routes do not prove physical transit residency

Application relay choices identify controlled endpoints, not every underlay router, transit provider, or backup path. If physical transit is a requirement, the deployment needs independently verifiable network constraints. QUIC connection migration or MPTCP is not a jurisdiction policy engine.

## 5. Audit reconciliation

Use `project.md` §3.5.3 as the invariant numbering authority. In particular, **INV-2 is majority durability**, **INV-3 is acknowledged-data preservation on leadership change**, and **INV-5 is exact mirror prefix**. Some milestone reports relabel these as local fsync, outbox capacity, or policy signing. A passing test under a relabeled invariant does not satisfy the original requirement.

Change broad statements such as “every invariant implemented,” “zero cross-border plaintext,” and “split-brain rejected” to the exact tested boundary, revision, and fault model. The reservation and corruption-handling improvements are real progress; they do not close unrelated integration gaps.

## 6. Research thesis worth testing

> Can a recoverable, signed class policy applied consistently to admission, replication, replay, and ownership changes reduce unauthorized data movement and recovery errors, while preserving explicitly chosen per-class durability and availability, compared with equivalently configured existing systems?

Evaluate static permitted routes before adaptive routes. Treat speed improvements, loss bounds, and coverage as results to measure, not consequences of the architecture diagram. [Configuration and experiment roadmap](CONFIGURATION_ROADMAP.md) defines the next work packages.
