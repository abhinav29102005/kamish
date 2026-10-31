# Loophole closure ledger — implementation stages 1–2

**Phase 0 proposal, 1 November 2026:** [Contract](../docs/CONTRACT.md) and
[fresh baseline](../audit/phase0_baseline_2026-10-31_204424Z/REPORT.md) are available.
At `77bb051`, workspace 102 pass; original probes 11 pass; extended probes four
pass / two fail. Contract/fixture/witness choices await approval; no new closure
is claimed. Historical stage results below retain their original scope.

**Subsequent fixes:** [Application resolution report](../audit/application_resolution_2026-11-01/REPORT.md)
tracks replication ACK/range/stream-contract fixes, CLI corrections and truthful
frontend simulation. Consensus ownership and trusted audit completeness remain open.

**Follow-up testing:** [Application test report, 1 Nov 2026](../audit/application_testing_2026-11-01/REPORT.md)
reconfirms the original 11 checks and adds six failing broader-contract probes.
The statuses below describe their original scoped fixes, not closure of those
additional requirements. Use `--lib` to run only the original 11 probes.

**Date:** 2026-10-31. Changes based on `e0cd260`, tested in the working tree.

**Status: in progress, not production-ready.** The 11 research probes now pass,
but some pass through safe rejection or a limited in-process implementation.
This is not evidence that all architectural findings have been closed.

## Completed work

### Stage 1 — integrity, authorization and operability

- Versioned deterministic policy-signature encoding sorts maps and set-valued
  fields while retaining ordered failover preferences and exact integers.
- Same-version different-content policies and implicit policy-ID changes are
  rejected; expiry is exclusive and checked for integer overflow.
- Audit format 2 hashes structured fields with a domain separator. Configured
  disk audit writes sync before reporting success. Write errors latch the writer
  and cause enforcement to fail closed; daemons no longer fall back to memory.
- Broker/CLI short-argument conflicts are fixed and tested.
- Fetch checks the stored class instead of trusting request relabeling. Topic
  strings containing path traversal or separators are rejected before file access.
- Relay checks use the signed allowlist. Cleartext persistence, promotion, replay,
  backup and telemetry checks cannot treat sealed-only destinations as cleartext.
- Route caches are separated by class/destination/payload representation. Retained
  paths must still occur in the freshly permitted candidate set; stale metrics,
  invalid measurements and mismatched destinations cannot select an old route.

### Stage 2 — local append, session and recovery correctness

- Per-partition operation locks serialize producer dedupe/append/publication and
  concurrent receiver appends. Lazy disk-log initialization runs only once per
  broker partition, instead of recovering a file outside the winning map entry.
- New broker records use a checksummed v2 envelope containing producer identity,
  epoch, sequence, origin, request restrictions and admitted destination targets.
- Local restart rebuilds producer sessions; a changed payload/class/restriction
  under the same producer identity/sequence is rejected. New producer epochs start
  at sequence zero. Log recovery restores the highest recorded partition epoch.
- `try_new()` discovers existing partition directories and reconstructs bounded
  pending replication work. Agent drain paths refill from the log in pages, and
  explicit handshake recovery errors are propagated.
- Recovery intersects persisted destinations with current policy rather than
  dropping the original narrowing rule. Unknown legacy context is not guessed.
- Missing partition watermarks start at zero, never another partition's maximum.
  Unscoped ACKs reject empty/multi-stream ambiguity. Reservation growth must fit
  the capacity limit, and failed commits release their reservations.
- Registered in-process ownership is held stable across produce, checked for the
  current region, and checked for lease expiry before admission and before ACK.
- **Sealed replication is contained by rejection** at produce/receive/recovery
  boundaries until a safe encryption and key-custody integration exists.

## Reproduced findings: exact closure scope

| Finding | Status | Evidence / remaining boundary |
| --- | --- | --- |
| L01 plaintext labeled sealed | **Contained, feature disabled** | Broker rejects sealed requirements and forged sealed flags before remote persistence. Actual AEAD, key management, escrow and encrypted replication remain open. The standalone legacy `kamish-seal` construction is not approved for use. |
| L02 fetch relabeling | **Counterexample fixed** | Request/record class mismatch rejects the whole fetch; actual record class is checked. Authenticated topic creation/tenant authorization is still needed. |
| L03 cross-partition ACK | **Counterexample fixed; lifecycle partial** | Topic/partition isolation and ambiguous-ACK tests pass. Epoch/generation-scoped durable cursor protocol and source identity binding are still needed. |
| L04 concurrent producer retries | **Counterexample fixed** | Eight synchronized retries append once; changed content rejects. Applies within a single broker instance, not independently opened processes. |
| L05 restart dedupe | **Fixed for new local v2 records** | Producer context is part of the synced record and recovered at startup. Replica wire envelopes do not yet transfer producer context, so promoted-region dedupe is still open. |
| L06 automatic recovery | **Fixed for discoverable local v2 streams; deployment partial** | Constructor discovers streams; refill is bounded by queue capacity and page count per read. Standalone agent/broker processes still need a shared API or co-located worker for live coordination. |
| L07 policy roundtrip signature | **Fixed** | Repeated roundtrips, map/set order, ordered preferences, legacy format rejection and equal-version conflicts are tested. Persistent rollback floor is separate and remains open. |
| L08 recovery drops StrictLocal | **Fixed for new v2 admission records** | Live narrowing works even without the optional hook; recovery cannot add destinations absent at admission. Custom hook-only policy updates need a unified persistent policy contract. |
| L09 broker ignores owner | **In-process counterexample fixed; distributed safety open** | Wrong/expired registered owner rejects; valid owner's epoch is used. Unregistered compatibility mode still assumes local epoch 1; metadata is not persistent or consensus-backed. |
| L10 ambiguous audit hashing | **Fixed for format 2** | Field-boundary tampering is detected. Trusted external checkpoints, multi-process writer ownership, bounded retention and suffix-truncation detection remain open. |
| L11 CLI panics | **Fixed** | Broker help and all ctl subcommands are tested; both previously failing help commands exit successfully. |

## Compatibility and operational changes

1. **Policies:** format 2 signatures use a new digest. Missing/unknown
   `signature_format` is rejected. Re-sign the reviewed source policy through the
   authorized signing workflow; do not change only its format field.
2. **Audit:** missing/unknown `format_version` is rejected without changing the
   file. Preserve legacy evidence separately and start a reviewed new log; do not
   rewrite historical hashes or present an old ambiguous log as format 2 evidence.
3. **Records:** v1 logs remain readable with their original checksum. New local
   producer records use v2 context. Outbound recovery skips v1/replica records
   without admission context and warns. Legacy restrictions and sessions cannot
   be reconstructed by inference; they require an explicit migration policy.
4. **Outbox APIs:** `commit()` and unscoped `advance_watermark()` return `Result`.
   Handle errors. Use `advance_partition_watermark()` for known topic/partition
   handshakes, especially before a queue contains any ranges.
5. **Initialization:** daemon code uses fallible `BrokerServer::try_new()`.
   Compatibility `new()` panics on initialization failure instead of hiding it.
6. **Sealed mode:** a sealed-only requirement produces
   `sealed_replication_not_configured`. No successful sealed replication claim is
   made. Do not use the old XOR-based library for sensitive data.
7. **Performance:** same-partition producer operations and ownership updates are
   intentionally serialized at the relevant safety boundaries. Audit fsync adds
   latency. Earlier benchmark numbers do not describe this version.

## Remaining work: every broader review category

| Next stage | Open work from LOOPHOLES_REVIEW.md | Required closure evidence |
| --- | --- | --- |
| 3 — authoritative topic/policy lifecycle | Durable authenticated topic→class binding, per-partition policy/generation contract, signed processing/backup/telemetry roles, persistent policy version/digest floor, signer rotation, policy-file/service bootstrap | Policy restart/rollback/key-rotation tests; unregistered and cross-tenant requests rejected; no silent reclassification |
| 3 — mirror semantics | Mixed-record filtering versus dense offsets; destination changes; preserve source envelope/version/checksum; source and epoch/generation in transfer identity; same-offset different-data retries; retention floor | Prefix equality and gap tests across policy changes, concurrent senders, re-created topics and multiple epochs |
| 4 — transport/deployment | mTLS with caller role/region binding, bounded frames/payloads/connections, deadlines and rate limits; one physical writer per partition; broker and separate agent coordination | Real multi-process integration, unknown-client rejection, byte limits and crash/race tests |
| 5 — consensus/durability | Regional replica quorum and applied AckMode; persistent metadata quorum; fail-closed unknown ownership; durable lease fencing and promotion frontier proof | Independent brokers/regions, majority loss, stale/future epochs, process pause and clock-skew scenarios, acknowledged-history checker |
| 5 — recovery/consumer lifecycle | Real common-prefix reconciliation and quarantine/truncation transaction; producer state across regional failover; epoch-aware durable consumer offsets | Crash at every transition; no premature ACK/visibility; explicit loss and duplicate bounds |
| 6 — storage/resource bounds | Segments/index/retention, log/audit memory bounds, serialized-size accounting, cursor durability, disk reserve, source data retention, fair paginated catch-up, lock ownership across processes | Long outage exceeding cache sizes; disk/full/OOM fault tests; no loss or starvation across classes |
| 6 — sealed copies | Replace the standalone XOR construction with vetted AEAD, authenticated metadata, nonce uniqueness, key residency, rotation and escrow, then enable runtime sealing | Crypto known-answer/tamper tests, no plaintext on actual egress/storage, home-loss restore drill |
| 7 — adaptive transport | Actual relay transfer, measured RTT/goodput/backlog, sample-window confidence, breaker transitions, retry jitter/deadlines and per-stream fairness | Real network-path capture, delay/loss/asymmetry, no-alternate reporting, oscillation metrics |
| 7 — evidence | Encoded/compressed/fragmented canary scanning with positive controls; audit checkpoints/rotation; cross-region ownership histories; real telemetry or clearly labeled simulation | Known-leak detection and complete run artifacts, not only isolated helper tests |
| Design decisions | Class/topic versus mixed-record semantics; pinned policy expiry; fault-domain-specific zero-loss claims; revoke versus purge; retention/legal hold; external side-effect semantics; physical versus logical transit | Written approved contracts, incompatible-config rejection, scenario evidence and stated limits |

The source-only findings are not all represented by the original 11 probes.
In particular, passing the registered-owner test is **not** split-brain prevention
between independent metadata stores, and rejecting sealed mode is **not** encrypted
regional durability.

## Verification

- `cargo test --workspace`: **99 passed**, including new in-tree regression suites.
- Independent research harness: **11 passed** after the documented API/containment
  adjustments below.
- `cargo run -p kamish-broker -- --help`: passed.
- `cargo run -p kamish-ctl -- produce --help`: passed.
- `git diff --check`: passed.
- Rustfmt and Clippy: **not run successfully**; `cargo fmt` / `cargo clippy` are
  unavailable in the installed toolchain.

Research-probe adjustments are explicit: the ACK probe uses the scoped partition
API; the sealed probe accepts explicit rejection only after asserting no local
append or queued transfer; the fetch probe narrows its setup to StrictLocal so
it tests fetch authorization independently of disabled sealed egress. No probe
was marked ignored or converted to `should_panic`.
