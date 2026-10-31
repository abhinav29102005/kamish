# Phase 2 — Item 2.1 Audit Report: Server Limits, TOML Config Loader & W14 Physical Lock

**Execution Timestamp:** 2026-11-01T05:25:00+05:30  
**Status:** COMPLETE (Item 2.1)

---

## 1. Item + Files Modified

### Implemented Capabilities:
1. **W14 Single Physical Writer Enforcement (`crates/kamish-log`):**
   - Implemented `PartitionLock` utilizing OS kernel non-blocking exclusive locking (`libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB)` on `.lock`).
   - Prevents multiple OS processes from concurrently opening or mutating the same partition log directory.
   - Rejects concurrent access immediately with `LogError::LockedByAnotherProcess(dir)`.
2. **Server-Side Limits (`cmd/kamish-broker`):**
   - Configurable `ServerLimits` with defaults: `max_frame_size = 4 MB`, `max_payload_size = 1 MB`, `max_connections = 1024`.
   - **Max Frame Size:** Network ingestion uses a bounded frame reader with `BufReader::take(max_frame_size + 1)` and `read_until(b'
')`. Frames exceeding the limit fail-closed with `BrokerError::FrameTooLarge` and immediately terminate the TCP connection.
   - **Max Payload Size:** `produce()` rejects records exceeding `max_payload_size` with `BrokerError::PayloadTooLarge`.
   - **Max Connections:** Connection limiting via `Arc<tokio::sync::Semaphore>`. When concurrent connection count exceeds `max_connections`, new incoming TCP connections are cleanly rejected with a structured error response and closed.
3. **Real Config File Loader (TOML) (`cmd/kamish-broker`):**
   - Implemented `BrokerConfig` in `cmd/kamish-broker/src/config.rs` with `from_file`, `from_toml`, and `to_toml`.
   - Updated `kamish-broker` binary CLI (`cmd/kamish-broker/src/main.rs`) to accept `--config <path>` to run standalone daemons without test fixtures.

### Files Modified:
- `Cargo.toml`: Added `toml = "0.8"` to `[workspace.dependencies]`.
- `crates/kamish-log/Cargo.toml`: Added `libc = "0.2"`.
- `crates/kamish-log/src/lib.rs`: Added `PartitionLock`, `LogError::LockedByAnotherProcess`, `PartitionLog::open` lock acquisition, and test `test_two_processes_opening_same_log_directory_reject`. Dropped handles in reopen recovery tests.
- `crates/kamish-log/tests/adversarial_durability.rs`: Explicitly dropped log handle before reopening.
- `crates/kamish-log/tests/append_io_failures.rs`: Explicitly dropped log handles before reopening.
- `cmd/kamish-broker/Cargo.toml`: Added `toml = { workspace = true }`.
- `cmd/kamish-broker/src/config.rs`: Added `BrokerConfig` and `ServerLimits` with TOML serde support and tests.
- `cmd/kamish-broker/src/lib.rs`: Integrated `ServerLimits`, bounded frame reader, connection semaphore, and payload size validation.
- `cmd/kamish-broker/src/main.rs`: Added `--config` argument and TOML config bootstrap.
- `cmd/kamish-broker/tests/outbox_commit_resilience.rs`: Dropped direct log handle before broker fetch.
- `cmd/kamish-broker/tests/server_limits_and_locks.rs`: Added 5 integration tests covering limits and W14 partition locking.

---

## 2. Tests Added + Exact Commands + Real Output

### Tests Added:
- `crates/kamish-log/src/lib.rs`:
  - `test_two_processes_opening_same_log_directory_reject`: Confirms that two handles opening the same directory fail with `LockedByAnotherProcess`, and releasing the first allows the second to succeed.
- `cmd/kamish-broker/src/config.rs`:
  - `test_default_config_roundtrip`
  - `test_custom_toml_parsing`
  - `test_invalid_toml_fails_closed`
  - `test_load_from_file`
- `cmd/kamish-broker/tests/server_limits_and_locks.rs`:
  - `test_two_broker_daemons_on_same_partition_log_reject`: Verifies two broker daemons attempting to open the same partition log reject fail-closed with `LockedByAnotherProcess` (W14).
  - `test_server_limits_max_payload_rejection`: Verifies produce requests exceeding 100 bytes reject with `PayloadTooLarge`.
  - `test_server_limits_max_frame_rejection`: Verifies framed TCP lines exceeding 256 bytes return an error response and terminate connection.
  - `test_server_limits_connection_limiter`: Verifies connection semaphore rejects 3rd connection when limit is 2, and accepts after disconnect.
  - `test_toml_config_daemon_loading_and_limits`: Verifies TOML parsing from file.

### Commands Run:
1. `cargo test -p kamish-log`: 26 passed, 0 failed.
2. `cargo test -p kamish-broker`: 51 passed, 0 failed.
3. `cargo test --workspace`: All workspace tests passing.
4. `cargo test --manifest-path early_research/probes/Cargo.toml`: 17/17 probes passing (11 unit + 6 gaps).
5. `cargo clippy --workspace --all-targets -- -D warnings`: Clean, 0 warnings.
6. `cargo fmt -- --check`: Clean formatting.

Evidence captured in `audit/phase2_item2_1/`.

---

## 3. What is Still Not Covered (Phase 2 Roadmap)

- **W09:** TLS 1.3 + mutual auth (`rustls`) for client-broker, broker-agent, broker-metadata. Unknown peers reject. (Next item: Item 2.2).
- **W10:** Authorized topic/tenant catalog; producers cannot choose another tenant's class. (Item 2.3).
- **W13:** Consumer fetch bound to authenticated consumer identity, region, and role. (Item 2.3).
- **W11/W12:** Daemons load verified policy and trusted keys at startup; durable policy-version floor; key rotation and revocation protocol. (Item 2.4).

---

## 4. Updated Status Matrix (project.md)

| Invariant / Requirement | Previous Status | Current Status | Notes |
|:---|:---|:---|:---|
| **W14** (Single physical writer / IPC) | Open | **Implemented & Tested** | `PartitionLock` kernel flock rejects concurrent writers with `LockedByAnotherProcess`. |
| **Server Limits** (Frame, payload, conns) | Open | **Implemented & Tested** | Bounded frame reader (4MB), produce payload limit (1MB), semaphore connection limit (1024). |
| **Config Loader** (TOML) | Open | **Implemented & Tested** | `BrokerConfig` loads from TOML file/string with `--config` flag. |
| **W09** (mTLS 1.3 / per-client identity) | Open | In Progress | Item 2.2 |
| **W10** (Tenant catalog isolation) | Open | Planned | Item 2.3 |
| **W11/W12** (Policy boot verify & key rotation) | Partial | Planned | Item 2.4 |
| **W13** (Authenticated consumer fetch) | Open | Planned | Item 2.3 |
| **NFR-02** (Fail-closed policy handling) | Verified | Verified | Unchanged |
| **INV-1** through **INV-8** | All 17 Probes Passing | All 17 Probes Passing | 100% green |
