# Phase 2 — Item 2.4 Audit Report: W11 Policy Boot Verification & Floor, W12 Key Rotation & Revocation

**Execution Timestamp:** 2026-11-01T05:52:00+05:30  
**Status:** COMPLETE (Item 2.4 & Phase 2 Complete)

---

## 1. Item + Files Modified

### Implemented Capabilities (W11 & W12):
1. **Daemon Startup Policy Boot Verification & Fail-Closed Behavior (W11):**
   - Implemented `load_trusted_keys_from_file` and `load_verified_policy_from_file` in `crates/kamish-policy/src/lib.rs`.
   - On daemon startup (`cmd/kamish-broker/src/main.rs`), if policy or trusted keys files are specified:
     - Verifies cryptographic signature against trusted issuer public keys.
     - Verifies policy freshness (`now < issued_at + ttl`).
     - Verifies policy version meets or exceeds the durable version floor.
     - If the signature is forged, the issuer is untrusted, or the policy is stale/corrupted, startup aborts immediately (fail-closed).
2. **Durable Policy-Version Floor (W11):**
   - Added `policy_version_floor` to `PolicyCache` with `with_version_floor`, `set_version_floor`, and `version_floor`.
   - Rejects any policy version strictly less than the durable floor (`PolicyError::VersionBelowFloor { floor, received }`).
   - Persists the version floor to durable storage (`{data_dir}/policy_floor.txt`) on startup and whenever a higher verified version is installed, surviving cold daemon restarts.
3. **Key Rotation & Cryptographic Revocation Protocol (W12):**
   - Implemented `KeyRevocationNotice` and `SignedKeyRevocation` in `crates/kamish-policy/src/lib.rs`:
     - Structured payload with `format_version`, `revoked_key_hex`, `reason`, `revoked_at_sec`, and `authorizer_pubkey_hex`.
     - Signed with Ed25519 by a trusted authorizer/root key over canonical SHA-256 digest `kamish-key-revocation-v1`.
   - In `PolicyCache`:
     - `revoke_key`: Validates authorizer signature, checks authorizer is not itself revoked, adds target key to `revoked_keys`, and immediately evicts/invalidates any active cached policy signed by the revoked key.
     - `rotate_key`: Atomically installs a new active key and applies key revocation for the retired key.
     - `update`: Rejects any policy signed by a revoked key (`PolicyError::KeyRevoked`).

### Files Modified / Added:
- `crates/kamish-policy/src/lib.rs`: Added `VersionBelowFloor`, `KeyRevoked`, `UntrustedAuthorizer`, `Io` error variants, `KeyRevocationNotice`, `SignedKeyRevocation`, `PolicyCache` floor and revocation tracking, and disk file loaders.
- `cmd/kamish-broker/src/main.rs`: Wired startup policy verification, trusted key loading, and durable policy floor persistence.
- `cmd/kamish-broker/tests/policy_boot_and_revocation.rs`: Added integration tests verifying boot failure on untrusted/corrupted policy, durable floor persistence across restart, and key revocation/rotation.

---

## 2. Tests Added + Exact Commands + Real Output

### Integration Tests (`cmd/kamish-broker/tests/policy_boot_and_revocation.rs`):
- `test_daemon_boot_fails_on_untrusted_or_tampered_policy`:
  - Valid policy boots cleanly (W11).
  - Untrusted policy issuer fails boot fail-closed.
  - Corrupted cryptographic signature fails boot fail-closed.
- `test_durable_policy_version_floor_persistence`:
  - Verifies policy below durable floor (v8 < floor 10) fails with `VersionBelowFloor`.
  - Verifies policy v12 advances floor to 12.
  - Verifies persisted floor on disk survives and protects subsequent boots.
- `test_key_revocation_and_rotation_protocol_full`:
  - Active policy signed by Key A installed in cache.
  - Signed revocation for Key A processed; active policy immediately evicted.
  - Attempt to update policy with revoked Key A rejected with `KeyRevoked`.
  - Rotated Key B installs new policy v2 successfully.

### Exact Commands Run:
1. `cargo test --test policy_boot_and_revocation`: 3/3 passed.
2. `cargo test --workspace`: 100% passed across all crates.
3. `cargo test --manifest-path early_research/probes/Cargo.toml`: 17/17 passed.
4. `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, clean.
5. `cargo fmt --check`: Clean formatting.

Evidence captured in `audit/phase2_item2_4/`.

---

## 3. Phase 2 Completion Status

All Phase 2 requirements are now **fully implemented and verified**:
- **W09:** TLS 1.3 + mutual auth (rustls) for client-broker, broker-agent, broker-metadata. Per-client identity, roles, topic/tenant authorization. Unknown peers reject. (Complete)
- **W10:** Authorized topic/tenant catalog; producers cannot choose another tenant's class. (Complete)
- **W11/W12:** Daemons load verified policy and trusted keys at startup; durable policy-version floor; key rotation and revocation protocol. (Complete)
- **W13:** Consumer fetch bound to authenticated consumer identity, region and role. (Complete)
- **W14:** One supported deployment contract with single physical writer; test that two processes opening same log reject. (Complete)
- Server-side limits on frame size, payload size, connections. (Complete)
- Config file loader (TOML). (Complete)
