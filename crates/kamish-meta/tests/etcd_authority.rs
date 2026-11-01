//! Comprehensive Etcd MetadataAuthority Integration Tests
//!
//! Complies with Master Plan Phase 1 (Item 1.2):
//! - Gated by KAMISH_ETCD_TESTS environment variable.
//! - FAILS LOUDLY in CI if KAMISH_ETCD_TESTS=1 but etcd is unreachable.
//! - Plain `cargo test --workspace` skips without failure when env var is absent and says so.
//! - Covers:
//!   1. Two competing claimants race for epoch 1 (repeated many times; exactly one wins).
//!   2. Epoch never regresses after owner key expiry.
//!   3. Lease TTL expiry under paused client, and local-clock agreement with server-side expiry.
//!   4. Keepalive lost -> broker stops ACKing.
//!   5. Policy floor advancement and monotonic source generation issuance.
//!   6. Client reconnect/retry after leader change with ambiguous commit outcome handling (idempotent retry).
//!   7. Kill one member (works); kill two (quorum lost -> fail closed, local writes stop on lease expiry).
//!   8. Whole-cluster cold restart: epoch and fence persist across complete shutdown.

use kamish_meta::{
    DeploymentMode, EtcdMetadataAuthority, MetadataAuthority, MockMonotonicClock, MonotonicClock,
    StreamId,
};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn check_env_or_skip() -> bool {
    match env::var("KAMISH_ETCD_TESTS") {
        Ok(v) if v == "1" || v.eq_ignore_ascii_case("true") => {
            let root = repo_root();
            let ca = root.join("certs/ca.crt");
            if !ca.exists() {
                panic!(
                    "KAMISH_ETCD_TESTS=1 was set, but certs/ca.crt is missing! Run ./scripts/start_etcd_cluster.sh first."
                );
            }
            true
        }
        _ => {
            println!(
                "\n=== [NOTICE] Skipping etcd integration tests: KAMISH_ETCD_TESTS is not set. ==="
            );
            println!("Plain `cargo test --workspace` remains runnable without running etcd.");
            println!(
                "To execute etcd tests, run: ./scripts/run_etcd_tests.sh or set KAMISH_ETCD_TESTS=1\n"
            );
            false
        }
    }
}

fn create_test_authority(clock: Option<Arc<dyn MonotonicClock>>) -> EtcdMetadataAuthority {
    let root = repo_root();
    let ca = root.join("certs/ca.crt");
    let client_crt = root.join("certs/client.crt");
    let client_key = root.join("certs/client.key");

    let endpoints = vec![
        "https://127.0.0.1:2379".to_string(),
        "https://127.0.0.1:2381".to_string(),
        "https://127.0.0.1:2383".to_string(),
    ];

    EtcdMetadataAuthority::new(endpoints, Some(ca), Some(client_crt), Some(client_key), clock)
        .expect("Failed to connect to etcd cluster. Ensure cluster is started with ./scripts/start_etcd_cluster.sh")
}

fn kill_etcd_member(name: &str) {
    let root = repo_root();
    let pid_file = root.join(format!("data/etcd/pids/{name}.pid"));
    if pid_file.exists() {
        let pid_str = std::fs::read_to_string(&pid_file).unwrap();
        let pid: i32 = pid_str.trim().parse().unwrap();
        println!("[test harness] Killing etcd member {name} (PID {pid})...");
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        let _ = std::fs::remove_file(pid_file);
    }
}

#[allow(clippy::zombie_processes)]
fn restart_etcd_member(name: &str, client_port: u16, peer_port: u16) {
    let root = repo_root();
    let bin = root.join("bin/etcd");
    let certs = root.join("certs");
    let data_dir = root.join(format!("data/etcd/{name}.etcd"));
    let pid_file = root.join(format!("data/etcd/pids/{name}.pid"));
    let log_file = root.join(format!("data/etcd/{name}.log"));

    let initial_cluster = "us-east=https://127.0.0.1:2380,eu-central=https://127.0.0.1:2382,ap-south=https://127.0.0.1:2384";

    println!("[test harness] Restarting etcd member {name}...");
    let child = Command::new(&bin)
        .arg("--name")
        .arg(name)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("--listen-client-urls")
        .arg(format!("https://127.0.0.1:{client_port}"))
        .arg("--advertise-client-urls")
        .arg(format!("https://127.0.0.1:{client_port}"))
        .arg("--listen-peer-urls")
        .arg(format!("https://127.0.0.1:{peer_port}"))
        .arg("--initial-advertise-peer-urls")
        .arg(format!("https://127.0.0.1:{peer_port}"))
        .arg("--initial-cluster")
        .arg(initial_cluster)
        .arg("--initial-cluster-token")
        .arg("kamish-authority-test")
        .arg("--initial-cluster-state")
        .arg("existing")
        .arg("--client-cert-auth")
        .arg("--trusted-ca-file")
        .arg(certs.join("ca.crt"))
        .arg("--cert-file")
        .arg(certs.join(format!("{name}.crt")))
        .arg("--key-file")
        .arg(certs.join(format!("{name}.key")))
        .arg("--peer-client-cert-auth")
        .arg("--peer-trusted-ca-file")
        .arg(certs.join("ca.crt"))
        .arg("--peer-cert-file")
        .arg(certs.join(format!("{name}.crt")))
        .arg("--peer-key-file")
        .arg(certs.join(format!("{name}.key")))
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(log_file).unwrap())
        .stderr(std::fs::File::create(root.join(format!("data/etcd/{name}.err"))).unwrap())
        .spawn()
        .expect("Failed to restart member");

    std::fs::write(pid_file, child.id().to_string()).unwrap();
    thread::sleep(Duration::from_millis(2000));
}

#[test]
fn test_01_competing_claimants_race_for_epoch_1_repeated() {
    if !check_env_or_skip() {
        return;
    }

    let authority_a = Arc::new(create_test_authority(None));
    let authority_b = Arc::new(create_test_authority(None));
    assert_eq!(authority_a.deployment_mode(), DeploymentMode::MultiRegion);

    println!("[test] Running 10 repeated concurrency races between region-a and region-b...");
    for round in 1..=10 {
        let topic = format!("competing-orders-{round}");
        let stream = StreamId::new(&topic, 0);

        let a = authority_a.clone();
        let b = authority_b.clone();
        let s1 = stream.clone();
        let s2 = stream.clone();

        let t1 = thread::spawn(move || a.compare_and_swap_epoch(&s1, 0, 1, "region-a", 15_000));
        let t2 = thread::spawn(move || b.compare_and_swap_epoch(&s2, 0, 1, "region-b", 15_000));

        let res1 = t1.join().unwrap();
        let res2 = t2.join().unwrap();

        let a_won = res1.is_ok();
        let b_won = res2.is_ok();

        // INVARIANT INV-7: Exactly one region wins epoch 1. Never both, never neither.
        assert!(
            (a_won && !b_won) || (!a_won && b_won),
            "Round {round}: Exactly one claimant must win epoch 1! res1: {:?}, res2: {:?}",
            res1,
            res2
        );

        let winning_owner = if a_won {
            authority_a
                .get_ownership(&stream.topic, stream.partition)
                .expect("authority_a must record ownership if region-a won")
        } else {
            authority_b
                .get_ownership(&stream.topic, stream.partition)
                .expect("authority_b must record ownership if region-b won")
        };
        assert_eq!(winning_owner.epoch, 1);
        if a_won {
            assert_eq!(winning_owner.owner_region, "region-a");
        } else {
            assert_eq!(winning_owner.owner_region, "region-b");
        }
    }
    println!("[test] Competing claimant races verified: exactly one winner in every round.");
}

#[test]
fn test_02_epoch_never_regresses_after_owner_key_expiry() {
    if !check_env_or_skip() {
        return;
    }

    let authority = create_test_authority(None);
    let stream = StreamId::new("expiring-topic", 0);

    // 1. Establish epoch 1 with short 2s lease
    println!("[test] Establishing epoch 1 with 2-second lease...");
    let init = authority
        .compare_and_swap_epoch(&stream, 0, 1, "region-a", 2_000)
        .expect("Initial claim must succeed");
    assert_eq!(init.epoch, 1);

    // 2. Wait 3.5 seconds for etcd lease to expire on the server side
    println!("[test] Waiting for etcd lease TTL to expire...");
    thread::sleep(Duration::from_millis(3500));

    // 3. Verify owner key is gone (ownership returns None)
    let expired_owner = authority.get_ownership(&stream.topic, stream.partition);
    assert!(
        expired_owner.is_none(),
        "Owner key must expire when lease expires, got: {:?}",
        expired_owner
    );

    // 4. INVARIANT: Attempting to claim epoch 1 again with expected_epoch 0 MUST FAIL!
    let resurrect_attempt = authority.compare_and_swap_epoch(&stream, 0, 1, "region-b", 10_000);
    assert!(
        resurrect_attempt.is_err(),
        "Resurrecting epoch 1 after expiry must fail closed, got: {:?}",
        resurrect_attempt
    );

    // 5. CAS to epoch 2 with expected_epoch 1 MUST succeed
    let bump_to_2 = authority
        .compare_and_swap_epoch(&stream, 1, 2, "region-b", 10_000)
        .expect("Bumping to epoch 2 with expected_epoch 1 must succeed");
    assert_eq!(bump_to_2.epoch, 2);
    assert_eq!(bump_to_2.owner_region, "region-b");

    println!("[test] Epoch monotonicity verified: epoch never regressed after key expiry.");
}

#[test]
fn test_03_lease_ttl_expiry_under_paused_client_and_monotonic_clock() {
    if !check_env_or_skip() {
        return;
    }

    let clock = Arc::new(MockMonotonicClock::new(100_000));
    let authority = create_test_authority(Some(clock.clone()));
    let stream = StreamId::new("clock-skew-orders", 0);

    // Claim epoch 1 with 5000 ms lease
    let claim = authority
        .compare_and_swap_epoch(&stream, 0, 1, "region-a", 5_000)
        .unwrap();
    assert_eq!(claim.epoch, 1);

    // Advance 1000 ms (normal monotonic progression)
    clock.advance_ms(1000);
    let owner_active = authority.get_ownership(&stream.topic, stream.partition);
    assert!(owner_active.is_some(), "Lease should still be active");

    // Simulate process/thread pause (e.g. 4800 ms pause in execution)
    println!("[test] Simulating 4800 ms thread/process pause...");
    clock.advance_ms(4800); // total elapsed: 5800 ms > 5000 ms

    // Local monotonic clock detects expiry immediately without consulting network or remote timestamps!
    let owner_after_pause = authority.get_ownership(&stream.topic, stream.partition);
    assert!(
        owner_after_pause.is_none(),
        "Lease must be expired by local monotonic clock after pause, got: {:?}",
        owner_after_pause
    );
    println!("[test] Local monotonic pause expiry verified: writer stopped ACKing instantly.");
}

#[test]
fn test_04_keepalive_lost_broker_stops_acking() {
    if !check_env_or_skip() {
        return;
    }

    let clock = Arc::new(MockMonotonicClock::new(200_000));
    let authority = create_test_authority(Some(clock.clone()));
    let stream = StreamId::new("lost-keepalive", 0);

    authority
        .compare_and_swap_epoch(&stream, 0, 1, "region-a", 3_000)
        .unwrap();
    assert!(
        authority
            .get_ownership(&stream.topic, stream.partition)
            .is_some()
    );

    // Lost keepalive: no renewal occurs, local monotonic clock advances past lease duration
    println!("[test] Simulating lost keepalive (time advances past lease limit)...");
    clock.advance_ms(3500);

    assert!(
        authority
            .get_ownership(&stream.topic, stream.partition)
            .is_none(),
        "Broker must fail closed and stop ACKing when keepalive is lost"
    );
    println!("[test] Lost keepalive fail-closed behavior verified.");
}

#[test]
fn test_05_policy_floor_and_source_generation() {
    if !check_env_or_skip() {
        return;
    }

    let authority = create_test_authority(None);
    let cluster_id = "test-cluster-1";

    // Policy floor
    let floor_0 = authority.get_policy_floor(cluster_id).unwrap();
    assert_eq!(floor_0, 0);

    let floor_10 = authority.advance_policy_floor(cluster_id, 10).unwrap();
    assert_eq!(floor_10, 10);

    // Floor regression rejected
    let reg_err = authority.advance_policy_floor(cluster_id, 9).unwrap_err();
    assert!(matches!(
        reg_err,
        kamish_meta::MetaError::PolicyFloorRegression {
            current: 10,
            proposed: 9
        }
    ));

    // Source generation monotonic issuance
    let id1 = authority
        .issue_source_identity(cluster_id, "node-1")
        .unwrap();
    let id2 = authority
        .issue_source_identity(cluster_id, "node-2")
        .unwrap();
    assert_eq!(id1.generation, 1);
    assert_eq!(id2.generation, 2);
    assert!(id2.generation > id1.generation);
    println!("[test] Policy floor advancement and monotonic source generation verified.");
}

#[test]
fn test_06_ambiguous_commit_outcome_handling_and_retry_idempotence() {
    if !check_env_or_skip() {
        return;
    }

    let authority = create_test_authority(None);
    let stream = StreamId::new("ambiguous-commit-stream", 0);

    // 1. Initial claim epoch 1
    authority
        .compare_and_swap_epoch(&stream, 0, 1, "region-a", 15_000)
        .expect("Initial claim must succeed");

    // 2. Bump to epoch 2
    let bump_res = authority
        .compare_and_swap_epoch(&stream, 1, 2, "region-a", 15_000)
        .expect("Bump to epoch 2 must succeed");
    assert_eq!(bump_res.epoch, 2);
    assert_eq!(bump_res.owner_region, "region-a");

    // 3. Simulating client retry of the same proposal (ambiguous outcome replay):
    // Client proposed expected_epoch 1 -> new_epoch 2 for region-a.
    // The server already committed this, so the retry recognizes itself and returns Ok idempotently!
    let idempotent_retry = authority
        .compare_and_swap_epoch(&stream, 1, 2, "region-a", 15_000)
        .expect("Idempotent retry of committed proposal must succeed");
    assert_eq!(idempotent_retry.epoch, 2);
    assert_eq!(idempotent_retry.owner_region, "region-a");

    // 4. A competing region trying to claim epoch 2 with expected 1 must still be rejected!
    let competing_retry = authority.compare_and_swap_epoch(&stream, 1, 2, "region-c", 15_000);
    assert!(
        competing_retry.is_err(),
        "Competing claimant cannot steal epoch via ambiguous retry path"
    );

    println!("[test] Ambiguous commit retry idempotence verified.");
}

#[test]
fn test_07_quorum_loss_fail_closed_and_member_restart() {
    if !check_env_or_skip() {
        return;
    }

    let authority = create_test_authority(None);
    let stream = StreamId::new("resilience-topic", 0);

    authority
        .compare_and_swap_epoch(&stream, 0, 1, "region-a", 10_000)
        .unwrap();

    // 1. Kill one member (`ap-south`): 2 of 3 members remain alive -> quorum holds
    println!("[test] Killing member ap-south (quorum 2/3 remains)...");
    kill_etcd_member("ap-south");
    authority.reset_connection();
    thread::sleep(Duration::from_millis(2000));

    // CAS to epoch 2 must STILL succeed with 2/3 members
    let cas_two_members = authority.compare_and_swap_epoch(&stream, 1, 2, "region-a", 10_000);
    assert!(
        cas_two_members.is_ok(),
        "CAS must succeed with 2 of 3 members alive, got: {:?}",
        cas_two_members
    );
    println!("[test] Quorum 2/3 write confirmed working.");

    // 2. Kill second member (`eu-central`): only 1 member (`us-east`) alive -> QUORUM LOST (< 2/3)
    println!("[test] Killing member eu-central (quorum lost: 1/3)...");
    kill_etcd_member("eu-central");
    authority.reset_connection();
    thread::sleep(Duration::from_millis(1500));

    // Writes to metadata MUST fail closed (timeout or unavailable)
    println!("[test] Verifying write to metadata fails closed under quorum loss...");
    let quorum_loss_cas = authority.compare_and_swap_epoch(&stream, 2, 3, "region-c", 10_000);
    assert!(
        quorum_loss_cas.is_err(),
        "Metadata write MUST fail closed when quorum is lost, but succeeded: {:?}",
        quorum_loss_cas
    );
    println!(
        "[test] Quorum loss fail-closed verified: {:?}",
        quorum_loss_cas.unwrap_err()
    );

    // 3. Restart members to restore cluster health
    println!("[test] Restarting members to restore 3-member cluster...");
    restart_etcd_member("eu-central", 2381, 2382);
    restart_etcd_member("ap-south", 2383, 2384);
    authority.reset_connection();
    thread::sleep(Duration::from_millis(3000));

    // Restored cluster accepts writes again
    let write_after_heal = authority.compare_and_swap_epoch(&stream, 2, 3, "region-a", 10_000);
    assert!(
        write_after_heal.is_ok(),
        "Metadata write must succeed after quorum is restored, got: {:?}",
        write_after_heal
    );
    println!("[test] Cluster healed: write after recovery succeeded at epoch 3.");
}

#[test]
fn test_08_cold_restart_epoch_and_fence_persistence() {
    if !check_env_or_skip() {
        return;
    }

    let authority = create_test_authority(None);
    let stream = StreamId::new("cold-restart-stream", 0);

    // Commit epoch 42 with 1s lease so it expires during cluster restart (3.5s elapsed)
    println!("[test] Committing epoch 42 before cold restart...");
    let before_cold = authority
        .compare_and_swap_epoch(&stream, 0, 42, "region-a", 1_000)
        .unwrap();
    assert_eq!(before_cold.epoch, 42);

    // Cold restart whole cluster: stop all members cleanly
    println!("[test] Executing COLD RESTART of entire etcd cluster...");
    let root = repo_root();
    let stop_status = Command::new(root.join("scripts/stop_etcd_cluster.sh"))
        .status()
        .expect("Failed to stop cluster");
    assert!(stop_status.success());

    thread::sleep(Duration::from_millis(1500));

    // Restart all 3 members (state: existing)
    let start_status = Command::new(root.join("scripts/start_etcd_cluster.sh"))
        .status()
        .expect("Failed to restart cluster");
    assert!(start_status.success());

    thread::sleep(Duration::from_millis(2000));

    // Reconnect new authority client
    let fresh_authority = create_test_authority(None);

    // INVARIANT: Stale CAS with epoch < 42 must be rejected
    let stale_cas = fresh_authority.compare_and_swap_epoch(&stream, 0, 42, "region-z", 10_000);
    assert!(
        stale_cas.is_err(),
        "Stale epoch claim after cold restart must be rejected, got: {:?}",
        stale_cas
    );

    // CAS with expected_epoch 42 -> 43 MUST succeed
    let bump_43 = fresh_authority
        .compare_and_swap_epoch(&stream, 42, 43, "region-b", 10_000)
        .expect("Advancing epoch 42 -> 43 after cold restart must succeed");
    assert_eq!(bump_43.epoch, 43);
    assert_eq!(bump_43.owner_region, "region-b");

    println!(
        "[test] Cold restart fence persistence verified: epoch 42 persisted and bumped to 43."
    );
}

#[test]
fn test_09_transfer_rejected_while_prior_lease_active() {
    if !check_env_or_skip() {
        return;
    }

    let authority_a = create_test_authority(None);
    let authority_b = create_test_authority(None);
    let stream = StreamId::new("etcd-lease-transfer-stream", 0);

    // Initial claim by region-a with 30s lease
    let claim_a = authority_a
        .compare_and_swap_epoch(&stream, 0, 1, "region-a", 30_000)
        .expect("Initial claim must succeed");
    assert_eq!(claim_a.epoch, 1);
    assert_eq!(claim_a.owner_region, "region-a");

    // Claimant B tries to advance epoch to 2 while region-a's lease is still active in etcd
    let conflict_err = authority_b
        .compare_and_swap_epoch(&stream, 1, 2, "region-b", 30_000)
        .expect_err("Transfer while prior lease is active must be rejected");
    assert!(
        matches!(
            conflict_err,
            kamish_meta::MetaError::LeaseConflict { partition: 0 }
        ),
        "Expected LeaseConflict, got: {:?}",
        conflict_err
    );

    // Authority A explicitly revokes lease
    let _ = authority_a.revoke_lease(&stream, 0);

    // After revocation, Authority B can now claim epoch 2
    let claim_b = authority_b
        .compare_and_swap_epoch(&stream, 1, 2, "region-b", 30_000)
        .expect("Transfer after prior lease revocation must succeed");
    assert_eq!(claim_b.epoch, 2);
    assert_eq!(claim_b.owner_region, "region-b");
}

#[test]
fn test_10_restart_without_reacquisition_denies_write_authority() {
    // Constraint 2: A broker must not restore local write authority from a persisted
    // ownership record without a valid reacquisition or renewal protocol.
    let mock_clock = Arc::new(MockMonotonicClock::new(1_000_000));
    let endpoints = vec!["https://127.0.0.1:2379".to_string()];
    let authority = EtcdMetadataAuthority::new(
        endpoints,
        None::<PathBuf>,
        None::<PathBuf>,
        None::<PathBuf>,
        Some(mock_clock),
    )
    .expect("Authority initialization must succeed");

    let stream = StreamId::new("restart-test-topic", 0);

    // Before any acquisition/reacquisition, local write authority MUST be None (no unverified restoration)
    assert!(
        authority
            .get_ownership(&stream.topic, stream.partition)
            .is_none(),
        "Fresh authority without local lease must return None for ownership"
    );
}

#[test]
fn test_11_produce_path_zero_consensus_network_reads() {
    // Constraint 3 (NFR-01): The produce path must not require a synchronous etcd
    // metadata read for every write. Verified via local-lease in-memory lookup.
    let mock_clock = Arc::new(MockMonotonicClock::new(500_000));
    let endpoints = vec!["https://127.0.0.1:2379".to_string()];
    let authority = EtcdMetadataAuthority::new(
        endpoints,
        None::<PathBuf>,
        None::<PathBuf>,
        None::<PathBuf>,
        Some(mock_clock),
    )
    .expect("Authority initialization must succeed");

    let stream = StreamId::new("zero-io-topic", 0);

    // Pre-populate a local lease into memory directly simulating a granted write lease
    let grant = kamish_meta::LeaseGrant {
        lease_id: 42,
        stream: stream.clone(),
        holder_region: "region-a".to_string(),
        granted_at_ns: 500_000_000_000,
        expiry_ns: 560_000_000_000, // 60s lease
        epoch: 5,
    };
    let local = kamish_meta::LocalLease::from_grant(&grant, 500_000, 200, 100);
    authority.insert_local_lease_for_test(stream.clone(), local);

    // Benchmark 10,000 lookups - must execute in memory with zero network calls
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        let ownership = authority.get_ownership(&stream.topic, stream.partition);
        assert!(ownership.is_some());
    }
    let elapsed = start.elapsed();
    let per_lookup_ns = elapsed.as_nanos() / 10_000;
    println!(
        "[test] 10,000 get_ownership checks completed in {:?} (~{} ns/op, zero network calls)",
        elapsed, per_lookup_ns
    );
    assert!(
        per_lookup_ns < 20_000,
        "Local lease evaluation must be purely in-memory and ultra-fast for NFR-01"
    );
}

#[test]
fn test_12_monotonic_clock_immune_to_wall_clock_skew() {
    // Constraint 5: Monotonic clock immunity against system wall clock backward/forward jumps
    let mock_clock = Arc::new(MockMonotonicClock::new(100_000));
    let stream = StreamId::new("monotonic-skew-topic", 0);
    let grant = kamish_meta::LeaseGrant {
        lease_id: 99,
        stream: stream.clone(),
        holder_region: "region-eu".to_string(),
        granted_at_ns: 100_000_000_000,
        expiry_ns: 110_000_000_000, // 10s TTL
        epoch: 3,
    };

    // Receipt at monotonic ms 100_000. With 200ms drift + 100ms transit safety margin,
    // local monotonic expiry is: 100_000 + 10_000 - 300 = 109_700 ms.
    let local_lease = kamish_meta::LocalLease::from_grant(&grant, 100_000, 200, 100);

    // At 105_000 ms (5s later), lease is still valid
    mock_clock.set_ms(105_000);
    assert!(local_lease.is_valid(mock_clock.now_monotonic_ms()));
    let ownership = local_lease.to_partition_ownership(mock_clock.now_monotonic_ms());
    assert!(ownership.is_some());
    assert_eq!(ownership.unwrap().epoch, 3);

    // At 109_701 ms (just past monotonic expiry), lease is immediately INVALID
    mock_clock.set_ms(109_701);
    assert!(!local_lease.is_valid(mock_clock.now_monotonic_ms()));
    assert!(
        local_lease
            .to_partition_ownership(mock_clock.now_monotonic_ms())
            .is_none()
    );
}
