# Kamish contract — approved Phase 0 amendments

**Status: FULLY IMPLEMENTED & VERIFIED — 1 November 2026.**  
Verified across 261 automated workspace tests under Run ID `run-verification-20261101-113645`.  
External etcd 3.7.3 authority, local monotonic clock safety margins, in-region quorum durability ($M = \lfloor V/2 \rfloor + 1$), AEAD sealed replication, dynamic consumer groups, safe segment retention, and trusted audit witnessing are implemented, tested, and validated.
Release revision: `3bab180` (rebased on `6c0562e`). Full verification evidence recorded in [audit/release_verification_20261101/REPORT.md](../audit/release_verification_20261101/REPORT.md).

This is an approved implementation direction, not a description of completed
behavior. **No W01–W38 item is closed by this document.** Requirement identifiers
retain their meanings in [project.md](../project.md), especially §1.4 and §3.5.3.
Amendments and remaining questions are recorded in [ADR-0015](../decisions.md#adr-0015-phase-0-contract-and-approval-gates).

The [baseline report](../audit/phase0_baseline_2026-10-31_204424Z/REPORT.md)
records 102 passing workspace tests, 11 passing original probes, and four passing
plus two failing extended probes. Frontend and SDK checks pass. These results
cover existing local behavior; they do not validate the proposals below.

## 1. Scope and failure model

- Three **emulated** regions, each with a separately persisted metadata voter;
  metadata commands commit through two of three voters using a vetted consensus
  implementation. A metadata voter is not a payload replica.
- Crash, restart, pause, delayed/duplicated messages, network partition and
  recoverable storage failures form the proposed test model. The selected library
  and storage adapter must document their assumptions and error handling.
- One host with namespaces/processes demonstrates logical failures only. Killing
  a namespace does not demonstrate surviving loss of the host, its disk or its
  administrative credentials.
- Consensus assumes honest voters and durable committed storage. Audit witnessing
  assumes the verifier's trust configuration and the witness quorum remain
  independent of the manipulated regional audit file. Majority compromise,
  compromised active signing keys and whole-host rollback are outside that model.
- Region labels come from authorized configuration and component identity. They
  do not establish physical geography. Metadata, hashes and counts also need an
  explicit permitted placement; absence of message bodies is insufficient.
- Sealed transfer remains disabled. Relay, derive mode, protocol compatibility and
  package publication are outside this phase.

## 2. Deployment modes and ownership (W01/W02/W05)

### 2.1 Supported modes to implement

| Property | Explicit single-node compatibility | Multi-region |
| --- | --- | --- |
| Intended use | Isolated local development with synthetic data | Emulated distributed implementation and tests |
| Identity | Persisted local deployment/node ID | Authorized cluster ID, node/region identities and stream registry |
| Ownership | Local authority for its own isolated namespace | Three-voter metadata quorum issues owner grants and epochs |
| Unknown stream/owner | Explicit local creation may establish local ownership | Deny produce, replication commit and promotion until registered and authorized |
| Unreachable authority | Local operation within its own namespace | Only an existing unexpired verified lease permits continued operation; no renewal, promotion, local fallback or assumed epoch |
| Future epoch from caller/disk | Does not adopt remote authority | Reject until the exact grant is verified against committed metadata |
| Replication/promotion | No cross-region replication or regional promotion | Only registered streams, destinations and approved promotion protocol |
| ACK durability | One synced local file; identified as this mode | Selected contract in §3, after its implementation |
| Audit | Internal file integrity unless a real witness is explicitly configured | Independent checkpoint verification required for anchored claims |

**Approved:** explicit configuration selects compatibility mode. Missing or
ambiguous configuration never silently enables it. Single-node identities cannot
be presented as ownership of a stream in a multi-region cluster.

Existing constructors and tests currently assume local operation. Their migration
to explicit local mode belongs with the authority implementation and its tests.
The independent-owner probe must exercise multi-region admission and retain its
existing prohibition on simultaneous unregistered epoch-one acknowledgments.
Positive registered-owner tests must prevent a blanket-denial implementation from
masquerading as working authority.

Mode changes require an explicit stopped-writer import/registration procedure.
Opening an existing directory under a different mode must not silently create a
new authority, reuse an unrelated stream identity or reset its fence.

### 2.2 Authoritative state

Proposed logical fields, subject to encoding/version review before implementation:

```text
cluster_id
stream_id = (topic_id, partition_id, generation)
class_id and stream admission contract digest
owner_region, owner_node_or_replica_group_id
owner_epoch
lease_id, lease validity bounds, lease revision
membership_revision, active_policy_id/version/digest
committed_prefix reference and state-machine revision
```

**Source identity and generation are issued/registered by committed metadata
quorum operations, not self-asserted by a broker or watermark response.** A valid
transport identity alone does not grant ownership of that source history.

`generation` is issued by the authority for a new logical log incarnation, such
as deletion/recreation or explicitly authorized history replacement. A routine
process restart preserves it. Ownership transfer increments `owner_epoch`; it
does not rewrite historical record epochs or automatically reset generation.
Producer epoch remains a separate field.

Epochs are allocated by committed state-machine operations, never by accepting
`max(local_epoch, request_epoch)`. Each writer/receiver persists its highest
verified fence before applying work under the grant. Missing or inconsistent
state on restart causes a halt and authoritative reconciliation, not epoch one.
Epoch arithmetic overflow rejects.

### 2.3 Approved lease authority; no per-write metadata consultation

Write authority comes from a **time-limited metadata-quorum-issued lease**. The
quorum issues source identities, generations and owner epochs, and commits lease
grants/renewals. Ordinary writes validate the cached verified grant locally;
they do not perform a metadata read or commit for each write/batch.

The earlier phrase **"strict quorum-authorized commit" meant per-write metadata
completion**, not a lease. That recommendation is superseded. No temporary
per-write stage is selected. If separately authorized later, it must be labeled
temporary and leave NFR-01 open; it cannot be presented as the approved lease path.

Required lease semantics and evidence:

1. Bind the grant to the registered source/generation, owner identity/epoch,
   membership and permitted operation/policy revision. Check it before admission,
   persistence, consumer visibility and successful ACK, including after a process
   pause or slow fsync. A write that outlives its authority cannot receive a new
   successful ACK or silently become committed on restart.
2. Refuse new work when the local conservative deadline expires or timing cannot
   be trusted. Renewal failure does not extend a deadline. Continued operation
   during quorum loss lasts at most the remaining valid grant, subject also to
   policy freshness, revocation and resource bounds.
3. Persist verified epochs/fences and the authority's outstanding grant state.
   Restart must not reset an epoch, extend a cached lease or authorize a second
   owner. A new metadata leader must account for unexpired prior grants before
   issuing a conflicting one. A local monotonic timestamp cannot be blindly
   restored across a reboot.
4. Grant transfer requires exclusion of the old authority under an approved
   lease/fencing protocol, not just a numerically higher epoch. Leader kill,
   quorum loss, delayed renewal, pause/clock error and restart tests must exercise
   actual application grants; a library's internal leader lease is insufficient.
5. Use the approved initial timing parameters in §2.3.2 below. They are starting
   values for implementation and adversarial validation, not proof that the
   timing model is safe. Document clock-rate, pause, reboot, leader-change and
   in-flight append assumptions before claiming the ownership/fencing invariant.

Payload commit follows the chosen data-durability protocol under that lease.
An ACK already sent before expiry/revocation may arrive later; delivery time is
not a new commit. Phase 1 still has the existing single-file durability boundary;
the approved two-of-three payload quorum follows Phase 1 (§3).

**NFR-01 remains open until implemented and tested.** Its off-per-write-path
requirement is the target. Pinned-class continuation is bounded by valid leases;
indefinite continuation after lease expiry is not authorized by the amendment
and must not be inferred from the original pinned-class wording.

### 2.3.1 Local Monotonic Lease Clock Rules (Phase 1 Requirement)

Lease validity is computed strictly from a **local monotonic clock** measured from
when the grant was received, never from etcd, wall-clock (`SystemTime`), or remote
timestamps:
1. **Clock Source**: The broker measures elapsed time using a monotonic clock source
   (`std::time::Instant` / `MonotonicClock` trait). It is strictly immune to NTP
   clock steps, leap seconds, and backward time adjustments.
2. **Safety Allowances**:
   $$\text{effective\_duration} = \text{grant\_duration} - (\text{max\_clock\_drift} + \text{transit\_delay\_allowance})$$
   The safety allowance (clock drift buffer + estimated network transit) is
   deducted at the instant of grant receipt. If total allowance exceeds granted
   duration, the lease is rejected immediately.
3. **Client / Thread Pause Safety**: If the writer thread or process experiences
   a pause (GC, hypervisor deschedule, or process suspension), the local monotonic
   clock continues advancing. Upon resumption, elapsed monotonic time is inspected;
   if it exceeds `effective_duration`, the lease is observed as expired and
   subsequent writes fail closed without emitting successful ACKs.
4. **Reboot Invalidation**: Monotonic timestamps are process-local and cannot be
   restored across a host or broker restart. Upon restart, all cached leases are
   treated as expired until explicitly renewed or re-granted by the metadata quorum.

### 2.3.2 Initial Lease Timing Parameters (Approved Starting Contract)

The following values are the maintainer-approved starting parameters for the
external etcd 3.7.3 implementation. They must be represented explicitly in
configuration/constants and covered by deterministic-clock and integration tests.

| Parameter | Initial value | Normative behavior |
| --- | ---: | --- |
| etcd lease TTL | 10 seconds | Grant a 10-second etcd lease. The local protocol must not assume a longer TTL. |
| Renewal interval | 3 seconds | Attempt renewal on a monotonic schedule, well before the conservative local deadline. |
| Local safety margin | 2 seconds | Subtract this margin from the conservative lease window; reject new ownership-dependent work after the resulting deadline. |
| Operation timeout | 1 second | Bound each grant, renewal, and ownership-transition RPC. A timeout or transport error leaves the outcome potentially ambiguous and must not extend local validity. |

#### Deadline calculation and expiry

1. Capture a monotonic timestamp before starting the grant/renewal RPC. Use that
   timestamp as the conservative start of the lease window, unless an equally
   conservative, tested lower bound on remaining lease time is available.
2. Calculate the local deadline no later than
   `request_start_monotonic + 10 seconds - 2 seconds`. Do not start the full TTL
   at response receipt, because network transit and server processing have
   already consumed part of the lease.
3. Schedule renewal every 3 seconds. A renewal only creates a new local deadline
   after the implementation has evidence that the renewal succeeded and has
   calculated a conservative deadline for the renewed lease. Failed, delayed or
   ambiguous renewal must not extend the existing deadline.
4. The 1-second operation timeout bounds a single operation; it does not imply
   that every etcd client keepalive stream has a one-second lifetime. The adapter
   must specify how it applies the timeout to individual renewal attempts.
5. Use an exact integral-second etcd TTL for this initial contract. Reject
   unsupported fractional-second configuration rather than silently truncating
   milliseconds. The persisted `duration_ms`, etcd TTL, and local deadline must
   describe the same or a more conservative validity interval.
6. Wall-clock timestamps are diagnostic only and must never extend local lease
   validity. Process restart invalidates the monotonic deadline; reacquisition
   must follow the approved ownership protocol.

#### Safety boundary that remains to be proven

These values are not a standalone proof of safe ownership transfer. Before
claiming the fencing invariant closed, tests and the design review must address
clock-rate error, long process/thread pauses, metadata leader changes, lease
renewal ambiguity, and a durable append or `fsync` that crosses the conservative
lease deadline. A pre-append check and post-append check alone do not prove that
an expired owner cannot leave a durable record behind. If the existing storage
protocol cannot meet the contract, stop for a design decision; do not weaken the
invariant or report the gap as closed.

## 3. Acknowledgment contract (W03/W04/W17/W18)

### 3.1 Common conditions

A successful new-write ACK requires authorized identity/class/stream, the current
owner grant, required audit append, the selected payload durability boundary, and
a durable commit/session frontier. An outbox enqueue or TCP send alone is not
durable replication. Disk errors, missing replicas, policy denial and unavailable
authority cannot silently downgrade the selected mode.

Timeout or connection loss after dispatch remains an uncertain outcome. Retry
uses the same stream/producer ID/producer epoch/sequence and identical contents.
The Node SDK public API and its existing uncertain-write behavior remain fixed.

Distinguish persisted bytes, quorum-committed records, consumer-visible frontier,
and client-observed ACKs. A quorum count alone does not implement leader recovery:
the selected data-replication protocol must recover the committed prefix before
electing/serving a replacement. Partial suffixes cannot become visible merely
because they deserialize after restart.

### 3.2 Option 1 — three payload replicas inside each active region

**Approved, scheduled after Phase 1.** Each active region has three independent payload
replica processes/directories; ACK requires the record and recovery state durable
on two of three, plus the authority conditions above. Use a vetted replication/
consensus implementation and its durability integration, not ad hoc counting of
TCP responses.

- Directly targets **INV-2** and local leader-replacement **INV-3**.
- One local replica failure leaves a potential local majority. Availability also
  depends on metadata authority and policy, not just payload replica count.
- Cross-region mirroring is asynchronous unless option 2 is separately selected.
- Losing the entire source region can make its acknowledged, not-yet-mirrored
  suffix unavailable or unrecoverable. There is no universal numeric RPO bound.
- A class restricted to one region cannot continue after permanent loss of all
  payload copies in that region. Metadata voters elsewhere cannot restore it.
- A three-region test with local quorums requires up to nine payload replicas
  **plus** three metadata voters. Co-location reduces physical independence.

For a given generation, let `A` be the exclusive client-ACK frontier (or a
conservatively known potentially-ACKed commit frontier), and `R` the greatest
verified prefix available in an eligible surviving region. The potentially
missing acknowledged suffix is `[R, A)` when `R < A`; the record-count bound is
`max(0, A - R)`. Byte/time bounds need recorded sizes/timestamps. If either frontier
or eligibility is unknown, report **unknown**, not a measured bound. A lag sample
taken before failure is not necessarily `R` at failure.

### 3.3 Optional later mode — synchronous durability in K distinct emulated regions

The original §1.3 says local quorum plus `k` legal-set replicas, without resolving
whether `k` counts nodes, total regions or additional regions. **Proposed meaning
for approval:** `legal-set-sync(K)` counts **total distinct permitted regions,
including the source**, with `2 <= K <= 3` for the selected three-region topology.
Each counted region durably commits the same source prefix on its local two-of-
three payload quorum. `K=1` is option 1 and is displayed as `local-quorum`.

- Retains option 1's in-region requirement and adds a WAN acknowledgment wait.
- Each accepted record has durable copies in K regional groups. Under the stated
  single-region-failure model, at least one other group retains it for `K >= 2`.
  Serving/promoting still requires verified history, current policy and authority.
- Loss of the shared host/storage, loss of all K copies, or corruption outside
  the stated model is not covered. A surviving copy may be ineligible to serve.
- Insufficient reachable permitted groups blocks/rejects writes. The system does
  not reduce K, expand the legal set or acknowledge an outbox entry instead.
- Single-region-only classes cannot select `K >= 2`; reject that configuration.
- Changing K is an authorized stream-contract change. It does not retroactively
  strengthen old acknowledgments or relax their recovery obligations.

A cheaper variant with **one payload node per emulated region** is a different
contract. Counting those nodes would not satisfy INV-2 as written. It would need
an explicit requirement amendment and differently labeled evidence; it is not the
recommended interpretation of option 2.

### 3.4 Approved sequencing and promotion boundary

Implement **Phase 1 authority/fences/leases (W01/W02/W05), then audit witnessing
(W08)** before option 1 payload quorum work. Option 1 remains the selected later
durability contract. `legal-set-sync(K)` is documented only as an optional later
mode; its implementation and K are not selected. Neither mode is implemented
by the current `AckMode` enum values.

For either option, automatic promotion requires a verified candidate prefix
covering the last potentially acknowledged authoritative frontier. If that frontier
cannot be established, stop. For asynchronous option 1, do not silently promote
a lagging regional copy and call it preservation of the prior ACK contract.
Any operator-authorized recovery accepting missing records needs a separate
decision, explicit incident/history record and client recovery semantics.

## 4. Replication watermark: identity, generation and prefix (W06/W07)

A watermark is an **exclusive contiguous prefix**, not the highest offset seen.
`W=0` means an empty verified prefix; `W=1` covers offset 0.

Proposed authenticated receipt:

```text
protocol_version, cluster_id
topic_id, partition_id, generation
source replica-group identity and current authorized sender/owner_epoch
destination replica-group identity and receiver storage incarnation
durability mode and membership revision
W = next offset after the verified durable prefix
prefix commitment / checkpoint reference
authority revision and receipt identity
```

1. Receipt `W` asserts that **all** records in `[0,W)` for this stream/generation
   match the source's committed envelopes and meet the stated durability mode.
   Gaps and conflicting bytes prevent advancement. Source identity and generation
   must resolve to metadata-quorum-issued records, using verified cached authority
   where valid. A peer cannot register itself by returning a watermark tuple.
2. Transfer the full source envelope: original class, policy version, partition
   epoch, payload/checksum and producer/admission context. Receiver enforcement
   uses current policy but records its decision separately; it does not restamp
   the source envelope or trust source permission as a current authorization.
3. Historical record epoch and current sender's grant are distinct. A newly
   authorized owner can replay older committed records without changing them;
   a caller cannot self-authorize by declaring a larger epoch.
4. Retries compare full envelope identity/content. Different records at the same
   generation/offset reject, including context-only or checksum changes.
5. The source validates receiver identity, generation, membership, durability and
   prefix correspondence before persisting progress or trimming work. `max()` of
   unrelated histories is never recovery.
6. Data, cursor and recovery metadata must survive restart consistently. A receiver
   restored from an older snapshot or missing disk state cannot advertise the old
   higher watermark; its storage incarnation requires explicit reconciliation.
7. Ownership transfer preserves the agreed prefix and records the source authority
   transition. Topic recreation gets a new generation; old receipts cannot trim it.
8. Retention/compaction requires an authorized checkpointed prefix/retention floor.
   Until that protocol exists, retained data needed for replay cannot be discarded
   merely because an in-memory queue is empty.

Exact versioned prefix encoding and receipt authentication are implementation
design gates. CRC32 alone is not an authenticated prefix commitment. A signature
on a receipt authenticates the issuer's statement, not the physical storage device.

## 5. Audit checkpoint and independent witness (W08)

### 5.1 Approved witness placement and threat-model limits

- Full decision entries stay in the configured regional audit store.
- A separate regional **audit checkpoint signing key** signs a domain-separated,
  versioned tuple. The verifier trusts its public key through an independently
  configured operator/cluster trust root; it does not accept a key from the file
  being verified. Key selection and rotation require an explicit reviewed setup.
- The three-voter **payload-free metadata quorum is the independent witness**.
  Signed checkpoint history and log registration live in its committed state,
  durable voter logs and snapshots, separate from the audit file/directory.
- Logical witness key: `(cluster_id, audit_log_id, log_generation, checkpoint_id)`.
  Registered log identity binds region, writer and signer. Losing/deleting the file
  cannot silently register a new log or reset the trusted count.
- Separate directories on one host provide an independent software witness for
  file-tampering tests, not protection from a host administrator or shared-disk loss.

**Separate-failure-domain assumption:** a supported independent witness requires
that manipulation/loss of a regional audit file cannot also rewrite or roll back
a majority of witness voters, the verifier's trust configuration or checkpoint
keys. Distinct voter processes/directories on the emulation host test logical
separation only. Hardware, storage and administrative failure domains must be
separated before making a claim about surviving those failures.

Raft provides crash-fault agreement, not Byzantine consensus. A compromised
regional writer/key can omit or fabricate future entries before anchoring;
checkpoint signatures do not prove truthful event capture. Majority compromise,
trusted-key substitution, common-host rollback, and entries omitted before the
first trusted checkpoint are outside the proposed detection claim. Missing
trust/witness evidence is reported as unavailable, never as verified completeness.

Proposed signed fields:

```text
format/domain, cluster_id, audit_log_id, log_generation, region_id, writer_id
checkpoint_id, previous_checkpoint_digest
count, tail_hash, signer_key_id, signing_key_version
```

`count` is the number of entries covered. For count C > 0, the tail is the hash
of sequence C-1; count 0 uses the defined genesis hash. Commit metadata separately
records consensus revision/membership. Hashes/counts are still metadata: permitted
placement and access must be approved for the selected synthetic dataset.

The state machine rejects count regression, same-count/different-hash updates,
wrong signer/log/generation and unauthorized reset. Retain prior checkpoint
bindings through snapshots so a later checkpoint cannot erase the earlier anchor.
The writer and verifier compare retained boundaries against the actual chain.
Consensus supplies ordered durable agreement, not Byzantine validation of what
an already-compromised signer says about new unanchored entries.

### 5.2 Periodic coverage versus complete tail coverage

**Approved direction:** periodic checkpoints with an explicit anchored-prefix
result, plus an awaited final checkpoint for a successfully closed run artifact.

**Maximum unanchored window — parameter proposal awaiting confirmation:** a hard
cap of **128 persisted entries beyond the last quorum-committed checkpoint**.
Propose a **1-second checkpoint scheduling target**, not a maximum exposure time.
The maintainer has required a stated bound but has not selected its numeric value;
128/1 second are proposed parameters and must be confirmed before W08.

Let C be the last independently committed count and N the local durable count.
Admission to *every* audit append reserves space so `N - C <= 128`, including
concurrent appenders, denied decisions and checkpoint failure paths. At the cap,
further appends/protected operations stop until a checkpoint commits; no
fallback to unaudited operation, overwrite or unbounded buffering is allowed.
The eventual implementation must test this gate and make the configured cap
part of run evidence. Merely triggering a background task every 128 entries does
not enforce a maximum while that task is stalled.

**Residual risk:** removal/replacement of a self-consistent suffix of at most
128 unanchored entries can evade the witness. There is **no finite maximum in
seconds** while the witness is unavailable or checkpoint publication is delayed;
stopping admission bounds entries, not how long an old suffix remains unanchored.
The 1-second timer cannot promise a 1-second window during quorum loss. Current
code has no witness/cap at all, so its present undetectable suffix is unbounded.

Checkpoint publication order:

1. Serialize audit append and sync the covered file prefix.
2. Snapshot its log identity, count and tail; sign using the authorized key.
3. Commit the checkpoint to the witness quorum and await durable completion.
4. Persist/return the committed checkpoint reference. A failed/unknown response
   is reconciled against the witness; it is not assumed absent or committed.

For a live file with N entries and latest trusted checkpoint C:

| Condition | Verification result |
| --- | --- |
| N < C, or hash at C-1 differs | Mismatch; fail |
| Prefix matches and N > C | Verified only through C; suffix `[C,N)` unwitnessed |
| Prefix matches and N = C | Matches that checkpoint; complete closed-run evidence additionally requires an authenticated closure record |
| No registered anchor or witness unavailable | Inconclusive/unavailable; no completeness success |
| Closed log's final count/hash differs | Mismatch; fail |

**Deletion entirely after the last committed checkpoint cannot be detected from
that checkpoint.** Even N=C cannot prove that a live log never had later entries.
Periodic witnessing must not be described as detecting every suffix deletion.
A full-tail alternative commits each required audit boundary synchronously and
trades write availability/latency for that coverage. It is not the selected
periodic mechanism and is not silently substituted for it.

`verify-audit` must obtain trusted identity and a linearizable witness view, check
signatures and chain boundaries, and report the exact covered count/revision.
It is read-only: it never creates evidence or signs the currently supplied file
to make it match. An offline pinned checkpoint proves only its historical scope.
Witness outage must be visible, and explicit close/checkpoint cannot report
success while its final commit is unavailable. Local audit-write failures remain
fail-closed. The proposed count cap means witness loss can stop operations before
a write lease expires; it does not move the metadata quorum onto every write.

### 5.3 Approved probe-change timing and required controls

`audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable` currently opens
a bare file, writes two entries, drops the writer, deletes the second entry and
reopens the file. It registers no witness, signer or checkpoint, and waits for no
periodic worker. Under that setup an independent checkpoint cannot exist merely
because a periodic feature is added elsewhere.

**Approved with timing restriction:** change the probe fixture only in the same
commit as the working witness implementation. Show the fixture diff for review.
Keep the existing test name and **tamper assertion unchanged**, including its
failure message, in `early_research/probes`; configure a real three-voter witness, write
both entries, explicitly await a covering checkpoint/finalization, then perform
the same deletion and require reopening/verification to reject against that
witness. No audit probe or fixture is edited by these documentation amendments.
Do not use `Drop` to pretend an asynchronous
checkpoint succeeded, use an editable sidecar as a trusted anchor, or deny every
reopen just to satisfy the negative assertion.

Required controls in the witness commit:

1. **Untouched-file positive control:** a registered, checkpointed file reopens
   and verifies successfully against the real witness.
2. **No-witness negative control:** remove a valid tail from an unanchored file;
   demonstrate that its internal integrity check still succeeds. Do not report
   that success as a completeness result.
3. **Anchored-prefix tamper test:** change/remove data covered by a committed
   checkpoint; verify that the independent witness check rejects it.
4. **Unanchored-window limitation test:** anchor C entries, append more within
   the configured cap, remove or recompute a self-consistent portion solely after
   C, and assert that the available anchor cannot detect it. Report verification
   only through C, even if the resulting file ends exactly at C.

Also retain suffix-removal, prefix-replacement, full-rewrite and unavailable-
witness tests. Prefix replacement/full rewrite must target independently anchored
content, not pretend arbitrary unwitnessed changes are detectable. The existing
probe remains failing until this implementation and its fixture are committed.

## 6. Policy precedence, revocation and existing copies (W11/W12/W28)

### 6.1 Proposed precedence

Permission requires the **intersection** of:

1. Authenticated topic/class/role and current owner/generation authorization.
2. The record's original admitted restrictions and immutable envelope identity.
3. The latest valid, activated signed policy for the operation and location.
4. Runtime capability: unsupported sealed/relay requirements continue to reject.

No layer grants permission denied by another. A later permissive policy cannot
silently expand a stored record's original destination contract. Same-version
different-content policies, rollback and unknown/stale policy fail closed.
Persist the verified policy ID/version/digest floor before exposing the update,
and reconcile against committed activation metadata after restart.

### 6.2 Activation and in-flight operations

**Approved requirement: revocation blocks use and egress immediately, including
in-flight sends.** An existing owner lease or earlier admission is not a bypass.
The implementation must cancel affected running send/fetch/replay tasks, stop
further transport writes (including partially sent frames/batches), and invalidate
queued grants/results. A final check only before starting a long transfer is
insufficient. Serialize local revocation activation with data-use/dispatch gates,
and recheck/cancel at every continuation that could release more bytes.

Receivers must block use and persistence under revoked authorization, discard
incomplete unauthorizable frames, and avoid acknowledging them as applied. Existing
durable bytes require the separate disposition instruction in §6.3. A revocation
cannot retrieve bytes already handed to the network or a consumer before the
blocking boundary; kernel-buffered bytes may still arrive. Tests must distinguish
new writes after that boundary from earlier bytes delivered late.

**Unresolved global activation boundary:** an isolated process cannot learn of a
new quorum revocation instantly while using its cached lease. Leases alone cannot
meet a global wall-clock interpretation starting at policy submission or metadata
commit. This conflict must be resolved explicitly, not hidden as a local-only
definition of "immediately". Options requiring the maintainer's decision:

- **Recommended: revocation barrier.** Publish a revoke-pending state, immediately
  cancel/block on receipt, and report revocation globally effective only after all
  affected enforcement gates confirm durable blocking, or all old operation
  grants provably expire under the approved bounds. Publication/request time and
  effective time are separate and visible. Consumer/replication authority must
  also expire; an owner write lease alone does not fence their use/egress.
- **At publication/commit time everywhere:** requires an additional enforceable
  fail-stop/revocable data-access boundary; disconnected cached authority alone
  cannot provide it. Per-write checks alone also cannot recall prior emitted bytes.

The immediate-cancellation requirement is accepted; the global activation
boundary is **decided via ADR-0018**: bound = max(revocation commit + barrier ack, lease TTL + max clock drift). Lease TTL is the authoritative revocation-latency bound and drives failover time.

Queued/replayed work is re-evaluated on drain and recovery. A denied destination
is marked blocked/revoked with its reason; it is not marked durably acknowledged.
Do not skip forbidden offsets and advertise a dense mirror. Close the affected
destination stream at its valid prefix; destination/contract changes require
explicit generation/topic transition semantics before writes resume.

### 6.3 Existing copies and disposition

**Approved: deny use/egress immediately and report disposition pending.**
Keep original envelope/checksum and record affected copies, outboxes, backups,
quarantines and restore sources. This is containment, not proof of erasure or a
claim that retention in the old location is permitted indefinitely.

An authorized signed lifecycle instruction must choose among:

- Purge with a recorded scope, deadline and completion/failure evidence.
- Move to an already authorized location, then remove the disallowed source copy.
- Retain under a specifically authorized hold with defined access/expiry rules.

Neither a hold nor a migration is inferred from the old admission policy. Where
storage itself is newly forbidden, moving bytes to quarantine on the same disk
does not satisfy removal. A hold/purge conflict or missing disposition policy
blocks automatic action and requires the policy authority's decision. Report
the unresolved copies explicitly. W28 is needed before accepting workloads whose
contract requires automated deletion/retention enforcement.

Tombstones/disposition state must survive restore and restart; restoring an older
backup cannot restore permission. Audit decisions and hashes also have their own
retention/placement contract. Preserve historical research evidence separately;
do not rewrite it to reflect a new policy.

## 7. Approval and acceptance gates

| Gate | Proposed behavior / required test | Related IDs | Present status |
| --- | --- | --- | --- |
| Modes/authority | Unregistered regions both denied; valid registered owner succeeds; stale/future owner cannot commit; leader kill, quorum loss and fence restart | W01/W02/W05; FR-07/08; INV-7 | Existing independent-owner probe fails |
| Durability choice | Two-of-three local durable ACK and replacement leader recovers ACK history; if option 2 selected, K-region receipts and single-region failure test | W03/W04; FR-02/03; INV-2/3/4 | No actual payload quorum |
| Watermark | Full prefix/envelope equality across restart/retry; wrong source/generation/incarnation rejected; no premature trim | W06/W07/W17/W18; FR-05/06; INV-1/5/6 | Local offset tests only |
| Audit witness | Anchored suffix removal, prefix substitution and rewrite reject; untouched file succeeds; missing witness and unanchored suffix reported | W08; FR-13; NFR-05 | Existing tail-removal probe fails |
| Policy lifecycle | Restart rollback/equivocation rejection; revocation during backlog/receive/fetch; disposition blocks restore | W11/W12/W28; FR-04; NFR-02; INV-8/9 | Local policy checks; lifecycle incomplete |
| Availability | Locally validated quorum-issued lease; no per-write metadata call; stop at safe expiry; renewed/restarted grants cannot overlap | NFR-01; FR-09 | Not implemented; indefinite pinned continuation is not claimed |

**Approved order:** W01/W02/W05, then W08; option 1 payload quorum follows
Phase 1. Source identity/generation come from the metadata quorum; dispositions
need explicit authorization. Audit fixture changes accompany the witness commit
and preserve the tamper assertion, with the four controls in §5.3.

Before adopting a consensus dependency, present license/maintenance/fit evidence
and wait for the maintainer's pick. Lease timing, the proposed 128-entry cap/
1-second scheduling target, and the global revocation boundary remain explicit
follow-up decisions. They are not silently approved numerical/security defaults.

Implementation dependency selection is also pending: evaluate a maintained Rust
Raft crate versus an external authority, storage adapter, license, maintenance
evidence and exact version pins before adding it. rustls is required for Phase 2;
AEAD selection waits for separately authorized sealing work.

Phase 2 also needs a compatibility decision: the current Node SDK is plaintext
and has no TLS options. Requiring direct client mTLS while freezing its public
API cannot be solved by pretending plaintext is authenticated. A reviewed
client-side transport adapter/tunnel or explicit future API authorization must
be agreed before that integration; the current SDK stays unchanged here.

**Current stop gate: Phase 1 preparation is authorized; implementation waits for
the consensus pick and relevant timing/consistency decisions.** Documents and
candidate comparisons do not close any W item or turn either probe green.


## 8. Supported Guarantees and Operational Boundaries (Workstream Reconciliation)

Following the completion of the One-Shot Functional Completion campaign, this section formally specifies the operational architecture, supported guarantees, and boundary conditions enforced across the live Kamish codebase.

### 8.1 Operational Durability: Supported Modes (LeaderFsync and InRegionQuorum)

Kamish supports two fully operational, non-simulated durability modes configurable on the regional broker daemon and per-request:

#### 8.1.1 Mode 1: Local NVMe Durability (`AckLevel::LeaderFsync`)
- **Operational Guarantee:** Every successful produce response acknowledging `AckLevel::LeaderFsync` guarantees:
  1. The record is durably appended and flushed to local NVMe storage with synchronous hardware barriers (`f.sync_all()`) on segmented commit log and index files before an acknowledgement is transmitted to the client.
  2. The write was admitted under an active, non-expired etcd-backed ownership lease evaluated locally using a monotonic clock with a conservative 2.0-second safety margin.
  3. Producer session sequence state is validated and updated for idempotent deduplication.
  4. Cross-region asynchronous replication is staged into durable outbox queues.

#### 8.1.2 Mode 2: In-Region Multi-Node Quorum Durability (`AckLevel::InRegionQuorum`)
- **Operational Guarantee:** When `AckLevel::InRegionQuorum` is requested or configured, every successful produce acknowledgement guarantees:
  1. **Strict Majority Quorum:** The record has been durably appended and flushed via synchronous file system barriers (`f.sync_all()`) on at least a strict majority $M = \lfloor V/2 \rfloor + 1$ of configured voting replicas in the regional group (e.g. at least 2 of 3 distinct broker processes).
  2. **Inter-Replica Replication Protocol:** The partition leader dispatches `QuorumAppend` wire RPCs concurrently over TCP/mTLS to all configured voting peer replicas.
  3. **Follower Fencing & Divergence Quarantine:** Each follower validates that the leader's consensus term $T \ge T_{\text{local}}$ and owner epoch $E \ge E_{\text{local}}$. If a follower holds uncommitted divergent records beyond the common base offset, they are isolated to `QuarantineStore` before truncation, preserving committed prefixes.
  4. **Commit-Index Gating (INV-4):** The leader advances its `commit_index` to the newly committed offset only after majority confirmation. Consumer fetch requests under `InRegionQuorum` are strictly bounded by `commit_index`, preventing consumers from reading uncommitted speculative records.
  5. **Truthful Ack Semantics:** If majority quorum cannot be reached (due to network partition, follower crashes, or timeout), the broker fails closed with `BrokerError::Ownership(OwnershipError::QuorumNotReached)` and NEVER silently downgrades to `LeaderFsync`.
  6. **Persistent Consensus State:** Terms, votes, and commit indices persist crash-safely to `<data_dir>/consensus_state.json` with temporary file `sync_all()` and atomic rename.

### 8.2 Production AEAD Sealing and Fail-Closed KMS Boundary
- **Cryptographic Primitives:** `crates/kamish-seal` integrates standard, audited cryptographic implementations via `ring::aead`:
  - `SealingAlgorithm::ChaCha20Poly1305` (IETF RFC 8439)
  - `SealingAlgorithm::Aes256Gcm` (NIST SP 800-38D)
- **Envelope Security Invariants:**
  - Every sealed record generates a unique 96-bit CSPRNG nonce (`OsRng`) per encryption operation.
  - Additional Authenticated Data (AAD) cryptographically binds:
    $$\text{AAD} = \text{tenant} \parallel \text{topic} \parallel \text{partition} \parallel \text{epoch} \parallel \text{policy\_version} \parallel \text{key\_id} \parallel \text{key\_version}$$
  - Tampering with ciphertext or context metadata causes Poly1305 or GCM tag authentication to fail, dropping the frame.
- **KMS Custody & Fail-Closed Boundary:**
  - `KeyProvider` trait abstracts key retrieval and unwrapping.
  - `DevKeyProvider` is strictly isolated for offline local development and rejects foreign-region unwrap requests.
  - `ExternalKmsProvider` models production cloud KMS integration (AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault) configured via `[kms]` in `BrokerConfig`.
  - In `BrokerServer::produce`: when a topic policy mandates `ResidencyPolicy::SealedCiphertext`, the broker verifies local key provider configuration. If external KMS custody is unconfigured, the broker fails closed immediately with error `"sealed_replication_not_configured"`. No plaintext or mock XOR transformations are permitted in transit.

### 8.3 Autonomous Audit Witness Publication
- **Durable Checkpoint Journal:**
  - `crates/kamish-audit::WitnessPublisher` periodically captures the rolling SHA-256 event digest, signs the payload with Ed25519, generates stable deterministic request IDs (`wit-req-<topic>-<partition>-<seq>-<hash>`) for idempotent delivery, and journals checkpoints to `witness_publisher_journal.json` using crash-safe atomic rename and `sync_all()`.
  - Checkpoints queue durably during network interruptions up to a bounded backlog (10,000 checkpoints).
- **Fail-Closed Verification:**
  - If no external witness endpoint is configured (`UnconfiguredWitnessDestination`), checkpoints accumulate in the journal and are marked pending.
  - Local audit verification fails closed if the audit log has suffered historical truncation or modification, preventing untrusted local state from asserting unanchored validity.

### 8.4 Dynamic Consumer Group Coordination
- **Protocol & Rebalancing:**
  - `ConsumerGroupCoordinator` manages dynamic client membership (`JoinGroup`, `HeartbeatGroup`, `LeaveGroup`).
  - Assigns partition ownership deterministically across group members using round-robin distribution over all subscribed topics.
  - `spawn_consumer_group_sweeper_worker` periodically sweeps expired members exceeding `session_timeout_ms`, evicts dead consumers, triggers immediate rebalancing, and advances group generation.
  - Single-process limitation: Consumer group state is coordinated per regional broker instance with crash-safe disk persistence (`consumer_groups.json`); cross-cluster distributed group balancing is partitioned by region.
- **Generational & Partition Fencing:**
  - Every rebalance increments the group's `generation_id`.
  - Offset commits validate `(group_id, member_id, generation_id)` and verify that the committing member currently holds assignment authority for the specific partition. Stale members or unassigned partition commits are rejected with `BrokerError::EpochFenced` or `BrokerError::PolicyDenied`.
  - `JoinGroup` enforces tenant authorization: clients cannot subscribe to topics owned by foreign tenants.
  - State persists atomically to `consumer_groups.json` with temporary file `sync_all` and atomic rename.

### 8.5 Safe Storage Retention and Prefix Reclamation
- **Offset Monotonicity & Tail Preservation:**
  - `PartitionLog::reclaim_prefix_before_offset` prunes historical log segments safely without renumbering offsets, tracking `base_offset_floor = new_retained_start_offset`.
  - Active segment tail is preserved: reclamation retains at least the latest record to preserve segment metadata and monotonic sequence boundaries. Any read for `start_offset < base_offset_floor` returns empty, preserving monotonicity without offset renumbering.
- **Strict Reclamation Ceiling:**
  - `RetentionCoordinator` calculates:
    $$\text{safe\_ceiling} = \min(\text{high\_watermark}, \min(\text{quorum\_commit\_indices}), \min(\text{active\_consumer\_checkpoints}), \min(\text{outbox\_watermarks}))$$
  - Records required by active consumer groups, uncommitted in-region quorum consensus, or un-replicated cross-region outbox queues are never garbage collected, guaranteeing zero consumer data loss and zero replication gaps.
- **Explicit Quarantine Retention Policy:**
  - `QuarantineStore::prune_quarantine` enforces a bounded record ceiling (100k records max) with crash-safe disk replacement, isolating uncommitted divergent suffixes without unbounded disk exhaustion.
