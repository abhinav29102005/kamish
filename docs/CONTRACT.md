# Kamish contract — Phase 0 proposal

**Status: PROPOSED — awaiting maintainer approval.** Prepared 1 November 2026.
Baseline revision: `77bb051bfa77e6be80e8d8e5f4ea3cd4dad9eba2`.

This is a proposed implementation contract, not a description of completed
behavior. **No W01–W38 item is closed by this document.** Requirement identifiers
retain their meanings in [project.md](../project.md), especially §1.4 and §3.5.3.
Conflicts requiring approval are recorded in [ADR-0015](../decisions.md#adr-0015-phase-0-contract-and-approval-gates).

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

### 2.1 Proposed supported modes

| Property | Explicit single-node compatibility | Multi-region |
| --- | --- | --- |
| Intended use | Isolated local development with synthetic data | Emulated distributed implementation and tests |
| Identity | Persisted local deployment/node ID | Authorized cluster ID, node/region identities and stream registry |
| Ownership | Local authority for its own isolated namespace | Three-voter metadata quorum issues owner grants and epochs |
| Unknown stream/owner | Explicit local creation may establish local ownership | Deny produce, replication commit and promotion until registered and authorized |
| Unreachable authority | Local operation within its own namespace | No fallback to local authority or an assumed epoch |
| Future epoch from caller/disk | Does not adopt remote authority | Reject until the exact grant is verified against committed metadata |
| Replication/promotion | No cross-region replication or regional promotion | Only registered streams, destinations and approved promotion protocol |
| ACK durability | One synced local file; identified as this mode | Selected contract in §3, after its implementation |
| Audit | Internal file integrity unless a real witness is explicitly configured | Independent checkpoint verification required for anchored claims |

**Recommendation:** explicit configuration selects compatibility mode. Missing or
ambiguous configuration never silently enables it. Single-node identities cannot
be presented as ownership of a stream in a multi-region cluster.

Existing constructors and tests currently assume local operation. Before Phase 1,
approve an explicit migration of local fixture setup to the named local mode.
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
membership_revision, active_policy_id/version/digest
committed_prefix reference and state-machine revision
```

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

### 2.3 Availability choice requiring approval

`project.md` NFR-01 promises a control plane off the ordinary write path and
pinned-class continuity. That differs from requiring immediate quorum knowledge
at every operation. Choose one explicitly:

| Option | Mechanism | Trade-off |
| --- | --- | --- |
| **A — strict quorum-authorized commit (recommended for the first multi-region implementation)** | Persist the payload under its chosen durability mode, then atomically validate owner/policy and commit the corresponding frontier/operation identity in metadata before ACK | Quorum loss stops new commits. This intentionally does not satisfy NFR-01; approval of that staged scope is required |
| B — cached quorum-issued leases/pinned grants | A separately specified grant protocol permits local commits until a safe grant boundary, with persistent fences and promotion exclusion | Can keep metadata off the per-write path, but needs approved clock/pause/restart bounds, renewal/revocation rules and a safety argument; a read of metadata followed by append is insufficient |

Option A uses the selected library's replicated state machine; it is not a new
consensus algorithm. An ownership read before fsync is **not** the final authority
check. A completion command must compare the expected owner epoch, generation,
policy revision and prefix atomically with other metadata commands.

The linearization point is the successful authoritative completion commit.
An operation committed before a fence/revocation may have an ACK delivered later.
A delayed response is not a new stale-epoch commit. A write that persists locally
but cannot complete authoritatively remains uncertain/uncommitted and must not
be exposed as committed data. Recovery distinguishes these states.

**Proposed initial choice: A, with NFR-01 explicitly pending.** Pinned continuity
under B is not inferred from single-node compatibility. No final lease durations,
clock bounds or destructive recovery policy are selected by this document.

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

**Recommended foundation.** Each active region has three independent payload
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

### 3.3 Option 2 — synchronous durability in K distinct emulated regions

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

### 3.4 Recommendation and promotion boundary

Approve **option 1 first** to implement the existing local quorum invariants and
make the WAN trade-off measurable. Select option 2 instead only if synchronous
regional durability and its deployment/latency cost are required now. Both are
proposals; neither is implemented by current `AckMode` enum values.

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
   Gaps and conflicting bytes prevent advancement.
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

### 5.1 Proposed location and trust

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

**Recommendation:** periodic checkpoints with an explicit anchored-prefix result,
plus an awaited final checkpoint for a successfully closed test/run artifact.
Checkpoint after whichever occurs first: a configured entry-count threshold or
time interval. Numeric defaults and maximum permitted unanchored backlog require
approval before implementation and must appear in every run configuration.

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
trades write availability/latency for that coverage. This is approval option D2
in ADR-0015, not an implicit change to the proposed periodic mechanism.

`verify-audit` must obtain trusted identity and a linearizable witness view, check
signatures and chain boundaries, and report the exact covered count/revision.
It is read-only: it never creates evidence or signs the currently supplied file
to make it match. An offline pinned checkpoint proves only its historical scope.
Witness outage must be visible, and explicit close/checkpoint cannot report
success while its final commit is unavailable. Local audit-write failures remain
fail-closed. Whether continued operation during witness outage has an age/count
cap, and what that cap is, is an explicit pending availability decision.

### 5.3 Existing probe integration needs approval

`audit_tail_removal_requires_a_trusted_checkpoint_to_be_detectable` currently opens
a bare file, writes two entries, drops the writer, deletes the second entry and
reopens the file. It registers no witness, signer or checkpoint, and waits for no
periodic worker. Under that setup an independent checkpoint cannot exist merely
because a periodic feature is added elsewhere.

**Proposed fixture change for approval:** retain the test name and suffix-removal
assertion in `early_research/probes`; configure a real three-voter witness, write
both entries, explicitly await a covering checkpoint/finalization, then perform
the same deletion and require reopening/verification to reject against that
witness. Include an untampered positive control, unavailable-witness tests and
tests of unwitnessed-tail scope. Do not use `Drop` to pretend an asynchronous
checkpoint succeeded, use an editable sidecar as a trusted anchor, or deny every
reopen just to satisfy the negative assertion.

If the fixture must remain byte-for-byte unchanged, that conflicts with explicit
external-witness setup and periodic coverage. Resolve it before Phase 1; the probe
remains failing now. Prefix replacement and whole-file rewrite tests must likewise
use real independently retained anchors and unchanged log identity.

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

**Recommendation:** revocation constrains subsequent operations; admission under
an earlier policy is not a permanent authorization for fetch/send/replay/restore.
Order activation against authorization/commit boundaries. Under §2 option A,
authoritative completion checks the active policy revision atomically; a stale
completion must re-evaluate or reject. Serialize each component's local policy
activation with its relevant persistence/dispatch boundary.

Bytes dispatched before revocation cannot be recalled. The receiver rechecks its
current activated policy before persistence. Record the activation, dispatch,
receive and commit boundaries rather than claiming an instantaneous global byte
recall. Disconnected components require either current quorum authorization or
an explicitly approved bounded grant; indefinite use of an old signed policy is
not permitted. Immediate global revocation with offline writes is not assumed.

Queued/replayed work is re-evaluated on drain and recovery. A denied destination
is marked blocked/revoked with its reason; it is not marked durably acknowledged.
Do not skip forbidden offsets and advertise a dense mirror. Close the affected
destination stream at its valid prefix; destination/contract changes require
explicit generation/topic transition semantics before writes resume.

### 6.3 Existing copies and disposition

**Recommended default: deny further use/egress and report disposition pending.**
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
| Availability | Approved quorum/grant behavior under control-plane loss | NFR-01; FR-09 | Strict option A would leave NFR-01 pending |

Before Phase 1, approve or amend: **modes, option 1 versus option 2 and K, strict
authority versus grants, periodic versus synchronous audit coverage and fixture
integration, and revocation/disposition precedence.** Checkpoint cadence/outage
budget and any lease parameters remain explicit follow-up decisions.

Implementation dependency selection is also pending: evaluate a maintained Rust
Raft crate versus an external authority, storage adapter, license, maintenance
evidence and exact version pins before adding it. rustls is required for Phase 2;
AEAD selection waits for separately authorized sealing work.

Phase 2 also needs a compatibility decision: the current Node SDK is plaintext
and has no TLS options. Requiring direct client mTLS while freezing its public
API cannot be solved by pretending plaintext is authenticated. A reviewed
client-side transport adapter/tunnel or explicit future API authorization must
be agreed before that integration; the current SDK stays unchanged here.

**Stop gate: Phase 1 is not authorized by this proposal.** Approval must resolve
the choices in ADR-0015; test names/counts or document text alone close no item.
