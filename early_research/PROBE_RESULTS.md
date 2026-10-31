# Research probe results

**Historical baseline below.** Subsequent stages now pass 11/11 probes; see
[LOOPHOLE_CLOSURE.md](LOOPHOLE_CLOSURE.md) for exact closure scope and test changes.
Sealed mode passes by explicit safe rejection, not successful encrypted transfer.

**Run date:** 2026-10-31

**Implementation revision:** `4ebfb74375ed2c6ba84e0fdbb4a0d864bb5cc86a`

**Environment:** Linux; `rustc 1.94.1 (e408947bf 2026-03-25) (Fedora 1.94.1-1.fc43)`; debug builds, local temporary directories.

## Results

The application workspace's **61 tests passed** with `cargo test --workspace`.
The independent [research harness](probes/src/lib.rs) ran **11 checks: 1 passed and 10 failed**. It asserts the intended behavior, so a failing assertion is a counterexample, not a success marker disguised as a test.

| Probe | Observed result | Finding |
| --- | --- | --- |
| `ack_of_one_partition_preserves_other_partition` | Pending count became 0; expected 1 after acknowledging only the other partition | L03 |
| `audit_hash_binds_field_boundaries` | Distinct structured field values generated the same digest | L10 |
| `broker_obeys_metadata_owner` | A acknowledged a write despite metadata assigning B, epoch 7 | L09 |
| `concurrent_retries_append_once` | 8 records persisted instead of 1 | L04 |
| `fetch_cannot_bypass_record_policy_with_another_class` | Public class request returned data denied under its stored class | L02 |
| `oversized_outbox_write_is_not_acknowledged` | **PASS:** oversized write was rejected | Previous admission bug fixed by atomic reservations |
| `pending_replication_survives_broker_restart` | Log had 1 record; fresh outbox had 0 pending ranges | L06, automatic recovery integration |
| `recovery_preserves_per_record_residency` | Recovery queued a record whose initial `StrictLocal` egress was denied | L08 |
| `retry_after_restart_is_idempotent` | Same retry returned new offset 1 instead of original offset 0 | L05 |
| `sealed_destination_does_not_persist_plaintext` | Recovered receiver record equaled the original canary payload | L01 |
| `signed_policy_survives_wire_roundtrip` | Equal decoded policy failed signature verification | L07 |

Separate debug CLI checks:

- `cargo run -p kamish-broker -- --help`: exit 101; duplicate `-h` for host/help.
- `cargo run -p kamish-ctl -- produce --help`: exit 101; duplicate `-p` for partition/payload. The policy option also needs a unique assignment.

## Reproduce

From the repository root:

```bash
cargo test --workspace
cargo test --locked --manifest-path early_research/probes/Cargo.toml --target-dir target -- --test-threads=1
```

Use `--offline` when dependencies are already cached. The research manifest has its own workspace boundary so these currently failing diagnostic probes do not silently become part of the application's existing suite. Its lockfile pins dependency resolution; **local path dependencies still follow the checked-out application revision**. Record that revision on every run.

To run one counterexample:

```bash
cargo test --locked --manifest-path early_research/probes/Cargo.toml --target-dir target recovery_preserves_per_record_residency -- --nocapture
```

The test fixture uses a fixed synthetic signing key, synthetic payloads and temporary directories. It does not open external network services or use real credentials.

## Scope and interpretation

- This is an API/disk-level harness, not an independent multi-region deployment, performance benchmark, power-loss experiment or exhaustive proof.
- The concurrent probe coordinates thread start but does not enumerate every scheduler interleaving. The recorded eight-append result is an observed counterexample; absence in another run would not prove atomicity.
- The policy fixture uses multiple hash-map/set entries to expose serialization-order instability. Fixes must cover nested sets/maps, not just pass one insertion order.
- The restart probe deliberately does not call `repopulate_outbox_from_log()`: it tests the missing automatic startup contract. In-tree tests that call that helper manually can pass simultaneously.
- The fetch probe changes the policy between broker instances to isolate authorization of existing records; it is not a test of the policy-update wire protocol.
- The sealed-copy probe forwards the same range payload/flag as the agent and inspects the receiver's disk record. It does not demonstrate a functioning encryption path.
- Once a defect is fixed, port the focused invariant test to its owning crate and update this table with a new revision/result. Do not keep calling a historical failure current.
