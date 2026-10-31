# Phase 1 consensus candidates

**Reviewed:** 1 November 2026. **Decision: awaiting maintainer pick.**
Contract amendments: `c516321`; [contract](CONTRACT.md), [ADR-0015](../decisions.md#adr-0015-phase-0-contract-and-approval-gates).
No dependency was installed, pinned in a manifest, or used to implement authority.

## Short comparison

| Candidate / exact version considered | License | Observed maintenance | Fit and work Kamish must supply |
| --- | --- | --- | --- |
| **OpenRaft `=0.9.25` — recommended** | MIT OR Apache-2.0; Apache-2.0 is an available choice [S1/S2] | Registry's highest non-prerelease: 0.9.25, published **2026-07-28**. Separate 0.10.0-alpha.37 published **2026-10-10**. The 0.9 branch accepts bug fixes; pre-1.0 API changes remain possible [S1–S3] | Async Rust/Tokio fit; typed application state machine, snapshots, membership and linearizable-read support. We still implement/review durable storage integration, RPC/rustls transport, partition leases/fences and checkpoint validation. Storage conformance tests supplement, not replace, crash tests [S2/S4] |
| **etcd `v3.7.3` — external authority** | Apache-2.0 [S7] | Release/changelog dated **2026-10-22**; recent maintenance releases include storage/concurrency and dependency fixes [S6] | Provides a separately operated replicated KV service, transactions, watches, TTL leases, persisted state and recovery tooling. Smaller consensus/storage integration burden, but needs a reviewed Rust client, three service processes, operational setup and a controlled metadata API. Native TTL leases/watch events do not automatically fence Kamish disk writes [S8/S9] |
| **TiKV raft-rs `raft = "=0.7.0"`** | Apache-2.0 [S5] | Latest published crate/release still **2023-03-07**. Repository has later fixes: newest observed master entry **2026-05-13**, `ad13f3d…`, addressing an unstable-entry buffer issue. Repository maintenance exists, but the published release lags [S5] | Rust core with TiKV usage documented upstream. Supplies the consensus module, not the persistent log, application state machine or transport. Most integration work here: event/Ready handling, persistence ordering, snapshots, transport and lease protocol. Using later fixes requires separately reviewing/pinning an exact Git revision [S5] |

### Recommendation

Pick **OpenRaft 0.9.25** for an embedded Rust metadata service and direct control
of the typed ownership/checkpoint state machine. Its documented Tokio/storage/
network interfaces fit this workspace. Use the non-prerelease line rather than
adopting 0.10 alpha by following a `latest` link. Recent alpha changes do not prove
that fixes are backported to 0.9; inspect relevant fixes before the adoption commit.

Pick **etcd** if reducing our consensus/storage implementation burden outweighs
running an external service. Its native leases are useful building blocks, but a
lost lease/delete notification does not stop a paused broker from writing by
itself. Durable epoch allocation, conservative local expiry and lease-exclusion
tests remain application work. etcd watches explicitly have **unbounded delivery
delay**, so they cannot implement immediate global revocation alone [S8].

raft-rs is a credible lower-level option, but its release lag and larger persistence/
network integration burden make it the least attractive for this phase. Do not
describe it as abandoned merely because the crate release is older.

## Common limits and adoption gates

- None of these supplies Kamish's approved partition lease protocol merely by
  electing its own consensus leader. Specify timing/renewal/reboot behavior and
  test leader death, quorum loss, delayed renewal and persisted fences.
- All three can host payload-free source/generation/epoch registration and signed
  audit checkpoint records. Application validation, signer authorization, durable
  monotonic floors and independent verifier trust are still required.
- etcd would be an external authority, not a Rust Raft library or a new Kamish
  gRPC compatibility feature. Its service-side TLS is its own implementation;
  it does not implement the required rustls client-broker/broker-agent paths.
- No candidate makes an isolated node learn a new revocation instantly. Resolve
  the barrier/effective-time question in ADR-0015 before implementing that claim.
- For the chosen Rust dependency, use an exact version/revision and tracked
  dependency resolution. For an external binary/image, record a verified artifact
  checksum/digest as well as version. Candidate versions here are **not adopted pins**.
- OpenRaft registry checksum for 0.9.25:
  `a97014fb78acb77be3a40ac2da305f6dd3a6b243f3a908ace87d29b3972eaafd`.
  raft-rs registry checksum for 0.7.0:
  `f12688b23a649902762d4c11d854d73c49c9b93138f2de16403ef9f571ad5bae`.
  These were read from registry metadata; crate archives were not downloaded or
  independently hashed in this review. etcd artifact digest is not yet verified.
- If raft-rs master is preferred, the observed commit is
  `ad13f3d90780f53aea2488c6a4b76c0d334bf136`; do not silently switch from 0.7.0
  to this revision. Review its dependency and persistence changes first.
- This is a source/fit review, not a benchmark, dependency vulnerability scan,
  complete transitive-license review or executed integration test. Maintainer
  activity is evidence of maintenance, not a proof of defect absence.

## Source register

All sources were retrieved during this review. Dates above identify registry
publication or displayed release/commit dates, not Kamish execution results.

| ID | Primary sources | Evidence used |
| --- | --- | --- |
| S1 | [OpenRaft registry](https://crates.io/api/v1/crates/openraft) | Version, publication timestamps, non-yanked status, license, checksum |
| S2 | [OpenRaft tagged 0.9.25 README](https://raw.githubusercontent.com/databendlabs/openraft/v0.9.25/README.md) | Tokio, interfaces, 0.9 bug-fix branch, pre-1.0 API caution, license choice |
| S3 | [OpenRaft alpha release](https://github.com/databendlabs/openraft/releases/tag/v0.10.0-alpha.37) | Recent activity and separate alpha line. GitHub's API marks this `prerelease: false`; its semantic version still contains `alpha` |
| S4 | [OpenRaft 0.9.25 integration guide](https://docs.rs/openraft/0.9.25/openraft/docs/getting_started/index.html) | Required storage/network adapters, durability responsibility and storage test suite |
| S5 | [raft registry](https://crates.io/api/v1/crates/raft), [0.7.0 release](https://github.com/tikv/raft-rs/releases/tag/v0.7.0), [commit history](https://github.com/tikv/raft-rs/commits/master/), [README](https://raw.githubusercontent.com/tikv/raft-rs/master/README.md), [tagged license](https://raw.githubusercontent.com/tikv/raft-rs/v0.7.0/LICENSE) | Published-release lag versus repository fixes; core-only integration responsibilities; Apache-2.0 |
| S6 | [etcd releases](https://github.com/etcd-io/etcd/releases), [3.7 changelog](https://raw.githubusercontent.com/etcd-io/etcd/main/CHANGELOG/CHANGELOG-3.7.md) | v3.7.3 release on 2026-10-22 and recent maintenance fixes |
| S7 | [etcd v3.7.3 license](https://raw.githubusercontent.com/etcd-io/etcd/v3.7.3/LICENSE) | Apache-2.0 |
| S8 | [etcd 3.7 API guarantees](https://etcd.io/docs/v3.7/learning/api_guarantees/) | Durable completed operations, transactional consistency, unbounded watch delay and uncertain client outcomes |
| S9 | [etcd 3.7 API](https://etcd.io/docs/v3.7/learning/api/) | Transaction/watch/lease APIs; TTL grants, keepalive and lease-attached key deletion |

Retrieval limits: several GitHub repository API calls returned HTTP 403, and
some raw-file requests failed. Public release/history pages and tagged files
above supplied the comparison; no unavailable repository metadata is inferred.
The tagged etcd changelog URL returned 404, so S6 uses the explicitly rolling
main-branch changelog. OpenRaft docs.rs displays a different date from the registry;
the publication date quoted above is the registry's `created_at` field. Before
adoption, verify the selected artifact and its support/security status again.

**Pick required:** OpenRaft 0.9.25, external etcd 3.7.3, or raft-rs (published
0.7.0 versus an explicitly reviewed Git revision). No implementation starts from
this recommendation alone.
