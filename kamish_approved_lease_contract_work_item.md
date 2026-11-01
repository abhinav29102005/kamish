# Kamish — Approved Lease Contract: Documentation Work Item

**Decision:** External etcd 3.7.3 selected. Initial lease parameters approved.  
**Baseline requested by maintainer:** `a133e26c4e80b2a1c7df559c75604defb29a4766`  
**Scope:** Documentation-only proposal for `decisions.md`, `docs/CONTRACT.md`, and `worktillnow.md`.  
**Runtime code changed:** No. **Tests run:** None; this is a documentation proposal.

## Repository access note

The connected GitHub repository is `abhinav29102005/kamish`, but the connected identity has read/pull permission and not push permission. I therefore did not commit to the repository or change its branch. The following edits are prepared for application and review in the local `E:\Kamish` checkout.

## 1. Approved initial parameters

| Parameter | Initial value | Required behavior |
|---|---:|---|
| etcd lease TTL | 10 seconds | Grant an exact 10-second etcd lease; do not assume a longer TTL. |
| Renewal interval | 3 seconds | Attempt renewal on a monotonic schedule, well before the conservative local deadline. |
| Local safety margin | 2 seconds | Stop admitting new ownership-dependent writes at the conservative deadline. |
| etcd operation timeout | 1 second | Bound each grant, renewal, and ownership-transition operation. A timeout does not prove that a renewal committed. |

These are approved starting parameters, **not a completed safety proof**. Audit cap and global revocation-effectiveness decisions remain separate open items.

## 2. Proposed `decisions.md` changes

### Decision Log Summary

Replace the ADR-0015 and ADR-0016 summary rows with:

```markdown
| **ADR-0015** | 2026-11-01 | Phase 0 contract and approval gates | **Approved with amendments; audit/revocation parameters remain pending** | §1.3–1.4, §2.4–2.6, §3.5.3; W01–W08/W11/W12/W28 |
| **ADR-0016** | 2026-11-01 | Phase 1 consensus candidate review | **External etcd 3.7.3 selected; implementation pending** | W01/W02/W05/W08; FR-07/08/13; NFR-01; INV-7 |
```

### ADR-0015 — Lease protocol bullet

Replace the existing unresolved numeric lease-default bullet with:

```markdown
- **Lease protocol:** the maintainer has approved the initial contract parameters
  recorded in ADR-0016 and `docs/CONTRACT.md`: 10-second etcd lease TTL, renewal
  every 3 seconds, 2-second local safety margin, and 1-second operation timeout.
  These are conservative starting parameters, not proof of safety; clock-rate,
  pause, restart, metadata leader-change, and in-flight append assumptions still
  require explicit analysis and adversarial tests. Persist outstanding grants
  across metadata leader changes; etcd's own leases do not by themselves implement
  Kamish partition ownership.
```

### ADR-0015 — Pending implementation choices

Replace the first bullet under `### Pending implementation choices` with:

```markdown
- **Consensus/metadata authority:** external etcd 3.7.3 has been selected by the
  maintainer. Adoption remains pending review of the exact etcd client version,
  transitive dependency graph, image/artifact digest, license notices, TLS
  configuration, and integration tests. This decision does not claim those
  checks are complete.
```

### ADR-0016 — Decision and parameters

Change the status to:

```markdown
**Status:** Decision selected; implementation and validation pending.
**Reviewed:** 1 November 2026.
```

Replace the existing `**Decision request:**` paragraph and its following two lines with:

```markdown
### Decision

The maintainer selected **external etcd 3.7.3** as Kamish's Phase 1 metadata
authority. Do not migrate this work item to OpenRaft or raft-rs without a new
explicit decision. etcd's transactions, durable KV state, and leases are building
blocks; they do not independently prove the correctness of Kamish's application
ownership protocol.

### Initial lease parameters approved for implementation and testing

| Parameter | Initial value | Required interpretation |
| --- | ---: | --- |
| etcd lease TTL | 10 seconds | Use an exact 10-second grant; do not silently convert an arbitrary millisecond duration by truncation. |
| Renewal interval | 3 seconds | Schedule renewal from a monotonic clock and retain the existing deadline unless renewal is confirmed safely. |
| Local safety margin | 2 seconds | Stop admitting new ownership-dependent writes at the conservative local deadline, before the etcd TTL can expire under the specified assumptions. |
| etcd operation timeout | 1 second | Bound each grant/renewal/ownership-transition operation; a timeout is not evidence that a renewal committed. |

The lease clock must be captured before the lease-grant/renewal request begins,
or otherwise use a rigorously justified conservative lower bound on remaining
lease time. The local deadline is not extended by a delayed response, retry,
watch notification, wall-clock adjustment, or ambiguous RPC result.

The 10/3/2/1-second values are approved initial parameters, not a completed safety
argument. The implementation must document and test clock-rate assumptions,
process pauses, restart behavior, etcd leader changes, lease-renewal ambiguity,
and durable appends crossing the local deadline. If these assumptions cannot
support the required fencing invariant, stop and return a design decision rather
than weaken the invariant.

The implementation order remains W01/W02/W05, then W08; payload quorum comes
after Phase 1. Audit cap and global revocation effectiveness remain separate
unresolved decisions.
```

Keep the candidate comparison above this decision text as historical evaluation evidence. Do not delete or rewrite its source observations.

## 3. Proposed `docs/CONTRACT.md` changes

### File header

Replace the current statement that no consensus candidate has been selected with:

```markdown
**Status: APPROVED WITH AMENDMENTS — 1 November 2026.** Implementation is pending.
External etcd 3.7.3 is selected as the Phase 1 metadata authority. Initial lease
parameters are approved as a starting contract: 10-second TTL, renewal every
3 seconds, 2-second local safety margin, and 1-second operation timeout. Audit
cap and global revocation parameters remain decision gates.
```

### Section 2.3, item 5

Replace the current unresolved numeric lease-parameter bullet with:

```markdown
5. Use the approved initial timing parameters in §2.3.2 below. They are starting
   values for implementation and adversarial validation, not proof that the
   timing model is safe. Document clock-rate, pause, reboot, leader-change and
   in-flight append assumptions before claiming the ownership/fencing invariant.
```

### Add after §2.3.1

```markdown
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
```

## 4. Proposed `worktillnow.md` status entry

Append a concise status entry near the existing Phase 1 authority progress section:

```markdown
**Phase 1 Decision Gate — Lease Contract (documentation proposal; no runtime behavior changed):**
- Maintainer selected external etcd 3.7.3 as the Phase 1 metadata authority.
- Maintainer approved initial lease parameters: 10-second etcd TTL, renewal every 3 seconds, 2-second local safety margin, and 1-second operation timeout.
- Proposed contract requires a conservative monotonic deadline starting no later than the grant/renewal request, no extension on ambiguous renewal, restart invalidation of cached monotonic lease state, and explicit treatment of in-flight durable appends crossing expiry.
- These values are initial parameters for implementation/testing, not a safety proof. Audit-cap and global revocation decisions remain open.
- This is a documentation-only work item. No runtime source, probes, historical audit evidence, or package publication state is changed. Tests are not claimed as run for this documentation proposal.
```

## 5. Verification and acceptance gate

This is documentation-only. After applying the edits locally:

1. Inspect the complete diff for only `decisions.md`, `docs/CONTRACT.md`, and `worktillnow.md`.
2. Confirm historical audit files and both known failing probes are unchanged.
3. Run `git diff --check`.
4. Do not claim runtime behavior or tests have been verified by this document change.
5. Commit this single documentation work item only after the diff is reviewed. The next item should be a separate deterministic test/reproduction commit, with no production-source changes until those tests and the design boundary are agreed.

No test output is included because no tests were run for this proposal.
