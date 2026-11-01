use kamish_audit::AuditLog;
use kamish_broker::{
    BrokerClient, BrokerConfig, BrokerError, BrokerRequest, BrokerResponse, BrokerServer,
    ProduceRequest, ServerLimits,
};
use kamish_enforce::Enforcer;
use kamish_meta::MetadataStore;
use kamish_policy::PolicyCache;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyDocument, SignedPolicy};
use rand::rngs::OsRng;
use std::collections::HashMap;

fn make_enforcer(region: &str, temp_dir: &tempfile::TempDir) -> Arc<Enforcer> {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pubkey_hex = hex::encode(signing_key.verifying_key().to_bytes());

    let mut legal = HashSet::new();
    legal.insert(region.to_string());
    legal.insert("EU_WEST".to_string());
    legal.insert("US_EAST".to_string());

    let mut modes = HashMap::new();
    modes.insert(region.to_string(), DestinationMode::Cleartext);
    modes.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
    modes.insert("US_EAST".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "global-data".to_string(),
        ClassRule {
            class_id: "global-data".to_string(),
            description: "Test".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec![],
            transit_allowed: true,
        },
    );

    let policy_doc = PolicyDocument {
        policy_id: "pol-test".to_string(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 5,
        ttl_seconds: 3600,
        issuer_pubkey_hex: pubkey_hex.clone(),
        classes,
        transit_rules: HashMap::new(),
    };

    let signed = SignedPolicy::sign(policy_doc, &signing_key);
    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex);

    let mut cache = PolicyCache::new(trusted);
    cache.update(signed, Utc::now().timestamp()).unwrap();

    let audit_log = AuditLog::open(temp_dir.path().join("audit.log")).unwrap();
    Arc::new(Enforcer::new(
        region,
        Arc::new(RwLock::new(cache)),
        audit_log,
    ))
}

fn setup_test_broker(
    limits: ServerLimits,
) -> (
    Arc<BrokerServer>,
    tempfile::TempDir,
    Arc<MetadataStore>,
    Arc<Enforcer>,
) {
    let temp_dir = tempfile::tempdir().unwrap();
    let enforcer = make_enforcer("EU_WEST", &temp_dir);
    let meta = Arc::new(MetadataStore::new());

    let broker = Arc::new(
        BrokerServer::try_new(
            "EU_WEST",
            Arc::clone(&enforcer),
            meta.clone(),
            vec!["US_EAST".to_string()],
            temp_dir.path(),
        )
        .unwrap()
        .with_limits(limits),
    );

    (broker, temp_dir, meta, enforcer)
}

#[tokio::test]
async fn test_two_broker_daemons_on_same_partition_log_reject() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path();

    let enf1 = make_enforcer("EU_WEST", &temp_dir);
    let meta1 = Arc::new(MetadataStore::new());

    // Broker 1 opens and produces to partition "orders-0"
    let broker1 = BrokerServer::try_new("EU_WEST", enf1, meta1, vec![], path).unwrap();
    let prod_res = broker1
        .produce(ProduceRequest {
            topic: "orders".to_string(),
            partition: 0,
            producer_id: 1,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".to_string(),
            payload: b"initial order".to_vec(),
            residency_rule: None,
        })
        .unwrap();
    assert!(
        prod_res.sovereignty_allowed,
        "Produce must succeed and write to disk"
    );

    let enf2 = make_enforcer("EU_WEST", &temp_dir);
    let meta2 = Arc::new(MetadataStore::new());

    // Broker 2 startup recovery scans data_dir and attempts to open "orders-0".
    // Because broker 1 holds the exclusive physical flock on orders-0, broker 2 must fail fail-closed with Log(LockedByAnotherProcess) (W14).
    let broker2 = BrokerServer::try_new("EU_WEST", enf2.clone(), meta2.clone(), vec![], path);
    assert!(
        matches!(
            broker2,
            Err(BrokerError::Log(
                kamish_log::LogError::LockedByAnotherProcess(_)
            ))
        ),
        "Second broker opening active partition log must fail with LockedByAnotherProcess (W14)"
    );

    // Dropping broker 1 releases the partition lock
    drop(broker1);

    // Broker 3 can now open the partition log and recover cleanly
    let broker3 = BrokerServer::try_new("EU_WEST", enf2, meta2, vec![], path);
    assert!(
        broker3.is_ok(),
        "Broker can recover partition log once previous instance has exited"
    );
}

#[tokio::test]
async fn test_server_limits_max_payload_rejection() {
    let limits = ServerLimits {
        max_payload_size: 100, // 100 bytes max
        max_frame_size: 4096,
        max_connections: 10,
    };
    let (broker, _td, _meta, _enf) = setup_test_broker(limits);

    // 1. Payload within limit (50 bytes) succeeds
    let valid_req = ProduceRequest {
        topic: "telemetry".to_string(),
        partition: 0,
        producer_id: 1,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "global-data".to_string(),
        payload: vec![b'A'; 50],
        residency_rule: None,
    };
    let resp = broker.produce(valid_req);
    assert!(resp.is_ok(), "Payload within max limit must succeed");

    // 2. Payload exceeding limit (150 bytes) is rejected fail-closed
    let oversized_req = ProduceRequest {
        topic: "telemetry".to_string(),
        partition: 0,
        producer_id: 1,
        producer_epoch: 1,
        sequence_base: 1,
        class_id: "global-data".to_string(),
        payload: vec![b'B'; 150],
        residency_rule: None,
    };
    let err = broker.produce(oversized_req).unwrap_err();
    assert!(
        matches!(
            err,
            BrokerError::PayloadTooLarge {
                size: 150,
                max: 100
            }
        ),
        "Payload exceeding 100 bytes must be rejected with PayloadTooLarge"
    );
}

#[tokio::test]
async fn test_server_limits_max_frame_rejection() {
    let limits = ServerLimits {
        max_payload_size: 1024,
        max_frame_size: 256, // 256 bytes frame limit
        max_connections: 10,
    };
    let (broker, _td, _meta, _enf) = setup_test_broker(limits);

    let (addr, _server_task) = broker.start_tcp_listener("127.0.0.1:0").await.unwrap();

    // Connect raw TCP client and send an oversized frame (> 256 bytes)
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let mut oversized_line = vec![b'X'; 500];
    oversized_line.push(b'\n');

    stream.write_all(&oversized_line).await.unwrap();
    stream.flush().await.unwrap();

    // Server must respond with an error message and terminate the connection
    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    let n = reader.read_line(&mut resp_line).await.unwrap();
    assert!(
        n > 0,
        "Server must send error response before terminating stream"
    );

    let resp: BrokerResponse = serde_json::from_str(&resp_line).unwrap();
    match resp {
        BrokerResponse::Error(msg) => {
            assert!(
                msg.contains("Frame size") && msg.contains("exceeds maximum allowed limit"),
                "Expected frame limit error message, got: {}",
                msg
            );
        }
        other => panic!("Expected BrokerResponse::Error, got {:?}", other),
    }

    // Subsequent read on the stream must encounter EOF because server closed the connection
    let mut eof_check = String::new();
    let eof_bytes = reader.read_line(&mut eof_check).await.unwrap();
    assert_eq!(
        eof_bytes, 0,
        "Connection must be closed by server after frame violation"
    );
}

#[tokio::test]
async fn test_server_limits_connection_limiter() {
    let limits = ServerLimits {
        max_payload_size: 1024,
        max_frame_size: 4096,
        max_connections: 2, // only 2 concurrent connections allowed
    };
    let (broker, _td, _meta, _enf) = setup_test_broker(limits);

    let (addr, _server_task) = broker.start_tcp_listener("127.0.0.1:0").await.unwrap();

    // 1. Connect client 1: succeeds
    let mut client1 = BrokerClient::connect(addr).await.unwrap();
    let ping1 = client1.send_request(BrokerRequest::Ping).await.unwrap();
    assert!(matches!(ping1, BrokerResponse::Pong));

    // 2. Connect client 2: succeeds
    let mut client2 = BrokerClient::connect(addr).await.unwrap();
    let ping2 = client2.send_request(BrokerRequest::Ping).await.unwrap();
    assert!(matches!(ping2, BrokerResponse::Pong));

    // 3. Connect client 3: connection limit (2) reached! Server rejects connection
    let raw3 = TcpStream::connect(addr).await.unwrap();
    let mut reader3 = BufReader::new(raw3);
    let mut line3 = String::new();
    let n = reader3.read_line(&mut line3).await.unwrap();
    assert!(n > 0);
    let resp3: BrokerResponse = serde_json::from_str(&line3).unwrap();
    match resp3 {
        BrokerResponse::Error(msg) => {
            assert!(
                msg.contains("connection limit reached"),
                "Expected connection limit error, got: {}",
                msg
            );
        }
        other => panic!("Expected BrokerResponse::Error, got {:?}", other),
    }

    // 4. Drop client 1 to release one connection permit
    drop(client1);
    // Allow permit cleanup to register
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // 5. Connect client 4: now succeeds because permit was freed
    let mut client4 = BrokerClient::connect(addr).await.unwrap();
    let ping4 = client4.send_request(BrokerRequest::Ping).await.unwrap();
    assert!(matches!(ping4, BrokerResponse::Pong));
}

#[test]
fn test_toml_config_daemon_loading_and_limits() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_file = temp_dir.path().join("daemon_broker.toml");

    let toml_content = r#"
region = "EU_NORTH"
host = "127.0.0.1"
port = 9099
data_dir = "./custom_data/broker"
peers = ["127.0.0.1:9098"]

[limits]
max_frame_size = 1048576
max_payload_size = 524288
max_connections = 512
"#;
    std::fs::write(&config_file, toml_content).unwrap();

    let cfg = BrokerConfig::from_file(&config_file).unwrap();
    assert_eq!(cfg.region, "EU_NORTH");
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 9099);
    assert_eq!(cfg.data_dir, "./custom_data/broker");
    assert_eq!(cfg.peers, vec!["127.0.0.1:9098"]);
    assert_eq!(cfg.limits.max_frame_size, 1048576);
    assert_eq!(cfg.limits.max_payload_size, 524288);
    assert_eq!(cfg.limits.max_connections, 512);
}
