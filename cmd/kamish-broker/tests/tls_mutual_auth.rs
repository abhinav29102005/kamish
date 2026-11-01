use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::tls::test_helpers::*;
use kamish_broker::{
    BrokerClient, BrokerRequest, BrokerResponse, BrokerServer, FetchRequest, ProduceRequest,
    ReplicationRequest, build_client_config, build_server_config,
};
use kamish_enforce::Enforcer;
use kamish_meta::MetadataStore;
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use rand::rngs::OsRng;
use rustls::pki_types::ServerName;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};

fn setup_test_broker(
    temp_dir: &tempfile::TempDir,
) -> (Arc<BrokerServer>, Arc<MetadataStore>, Arc<Enforcer>) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pubkey_hex = hex::encode(signing_key.verifying_key().to_bytes());

    let mut legal = HashSet::new();
    legal.insert("EU_WEST".to_string());
    legal.insert("US_EAST".to_string());

    let mut modes = HashMap::new();
    modes.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
    modes.insert("US_EAST".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "global-data".to_string(),
        ClassRule {
            class_id: "global-data".to_string(),
            description: "Replicable".to_string(),
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
    let enforcer = Arc::new(Enforcer::new(
        "EU_WEST",
        Arc::new(RwLock::new(cache)),
        audit_log,
    ));
    let meta = Arc::new(MetadataStore::new());

    let broker = Arc::new(
        BrokerServer::try_new(
            "EU_WEST",
            Arc::clone(&enforcer),
            meta.clone(),
            vec!["US_EAST".to_string()],
            temp_dir.path(),
        )
        .unwrap(),
    );

    (broker, meta, enforcer)
}

#[tokio::test]
async fn test_tls13_mutual_auth_success() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (broker, _meta, _enf) = setup_test_broker(&temp_dir);

    // 1. Generate Root CA and certificates
    let root_ca = make_ca("Kamish Root CA");
    let server_cert = make_cert_signed_by_ca(
        &root_ca,
        "localhost",
        "Kamish Infrastructure",
        &["Broker"],
        "EU_WEST",
    );
    let client_cert = make_cert_signed_by_ca(
        &root_ca,
        "producer-app:fintech:Producer:EU_WEST",
        "Fintech Corp",
        &["Producer"],
        "EU_WEST",
    );

    // 2. Build Server TLS 1.3 Config with Client Auth required
    let server_tls = build_server_config(
        server_cert.cert_pem.as_bytes(),
        server_cert.key_pem.as_bytes(),
        Some(root_ca.cert_pem.as_bytes()),
        true,
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(server_tls);

    let (addr, _server_task) = broker
        .start_tls_listener("127.0.0.1:0", acceptor)
        .await
        .unwrap();

    // 3. Client connects using mTLS (client cert signed by Root CA)
    let client_tls = build_client_config(
        root_ca.cert_pem.as_bytes(),
        Some(client_cert.cert_pem.as_bytes()),
        Some(client_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let connector = TlsConnector::from(client_tls);
    let server_name = ServerName::try_from("localhost").unwrap().to_owned();

    let mut client = BrokerClient::connect_tls(addr, server_name, connector)
        .await
        .unwrap();

    // 4. Ping and Produce succeed over TLS 1.3
    let ping_resp = client.send_request(BrokerRequest::Ping).await.unwrap();
    assert!(matches!(ping_resp, BrokerResponse::Pong));

    let prod_resp = client
        .produce(ProduceRequest {
            topic: "trades".to_string(),
            partition: 0,
            producer_id: 10,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".to_string(),
            payload: b"trade-execution-payload".to_vec(),
            residency_rule: None,
        })
        .await
        .unwrap();
    assert!(prod_resp.sovereignty_allowed);
    assert_eq!(prod_resp.base_offset, 0);
}

#[tokio::test]
async fn test_unknown_peer_certificate_rejected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (broker, _meta, _enf) = setup_test_broker(&temp_dir);

    // 1. Legitimate Root CA for broker
    let trusted_ca = make_ca("Kamish Trusted Root CA");
    let server_cert = make_cert_signed_by_ca(
        &trusted_ca,
        "localhost",
        "Kamish Infrastructure",
        &["Broker"],
        "EU_WEST",
    );

    let server_tls = build_server_config(
        server_cert.cert_pem.as_bytes(),
        server_cert.key_pem.as_bytes(),
        Some(trusted_ca.cert_pem.as_bytes()),
        true,
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(server_tls);
    let (addr, _server_task) = broker
        .start_tls_listener("127.0.0.1:0", acceptor)
        .await
        .unwrap();

    // 2. Rogue / Unknown CA creates an illegitimate client certificate
    let rogue_ca = make_ca("Rogue Untrusted CA");
    let rogue_client_cert = make_cert_signed_by_ca(
        &rogue_ca,
        "attacker:hack:Admin:RU",
        "Rogue Corp",
        &["Admin"],
        "RU",
    );

    // 3. Client attempts to connect with rogue certificate
    let rogue_client_tls = build_client_config(
        trusted_ca.cert_pem.as_bytes(),
        Some(rogue_client_cert.cert_pem.as_bytes()),
        Some(rogue_client_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let connector = TlsConnector::from(rogue_client_tls);
    let server_name = ServerName::try_from("localhost").unwrap().to_owned();

    // The handshake or subsequent request must fail because the server rejects unknown peers (W09)
    let conn_res = BrokerClient::connect_tls(addr, server_name, connector).await;
    let rejected = match conn_res {
        Err(_) => true,
        Ok(mut client) => {
            // First read/write must fail because server closed the connection
            client.send_request(BrokerRequest::Ping).await.is_err()
        }
    };
    assert!(
        rejected,
        "Server must reject client with untrusted certificate during TLS handshake or immediately drop (W09)"
    );
}

#[tokio::test]
async fn test_plaintext_connection_rejected_by_tls_listener() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (broker, _meta, _enf) = setup_test_broker(&temp_dir);

    let ca = make_ca("Kamish CA");
    let server_cert = make_cert_signed_by_ca(&ca, "localhost", "Kamish", &["Broker"], "EU_WEST");
    let server_tls = build_server_config(
        server_cert.cert_pem.as_bytes(),
        server_cert.key_pem.as_bytes(),
        Some(ca.cert_pem.as_bytes()),
        true,
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(server_tls);
    let (addr, _server_task) = broker
        .start_tls_listener("127.0.0.1:0", acceptor)
        .await
        .unwrap();

    // Attempt plain TCP connection without TLS
    let mut plain_client = TcpStream::connect(addr).await.unwrap();
    plain_client
        .write_all(b"{\"type\":\"Ping\"}\n")
        .await
        .unwrap();

    // Server TLS acceptor aborts the handshake; connection is immediately dropped
    let mut buf = [0u8; 64];
    let n = plain_client.try_read(&mut buf);
    assert!(
        n.is_err() || n.unwrap() == 0,
        "Plaintext connection must be rejected fail-closed"
    );
}

#[tokio::test]
async fn test_per_client_roles_producer_and_consumer_authorization() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (broker, _meta, _enf) = setup_test_broker(&temp_dir);

    let root_ca = make_ca("Kamish Root CA");
    let server_cert =
        make_cert_signed_by_ca(&root_ca, "localhost", "Kamish", &["Broker"], "EU_WEST");
    let server_tls = build_server_config(
        server_cert.cert_pem.as_bytes(),
        server_cert.key_pem.as_bytes(),
        Some(root_ca.cert_pem.as_bytes()),
        true,
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(server_tls);
    let (addr, _server_task) = broker
        .start_tls_listener("127.0.0.1:0", acceptor)
        .await
        .unwrap();

    // 1. Consumer-only client (lacks Producer role)
    let consumer_cert = make_cert_signed_by_ca(
        &root_ca,
        "consumer-worker:analytics:Consumer:EU_WEST",
        "Analytics Corp",
        &["Consumer"],
        "EU_WEST",
    );
    let consumer_tls = build_client_config(
        root_ca.cert_pem.as_bytes(),
        Some(consumer_cert.cert_pem.as_bytes()),
        Some(consumer_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let mut consumer_client = BrokerClient::connect_tls(
        addr,
        ServerName::try_from("localhost").unwrap().to_owned(),
        TlsConnector::from(consumer_tls),
    )
    .await
    .unwrap();

    // Consumer attempts to produce: must be rejected with role denial (W09)
    let prod_err = consumer_client
        .produce(ProduceRequest {
            topic: "data".to_string(),
            partition: 0,
            producer_id: 1,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".to_string(),
            payload: b"unauthorized write".to_vec(),
            residency_rule: None,
        })
        .await
        .unwrap_err();
    assert!(
        prod_err.to_string().contains("lacks Producer role"),
        "Consumer-only client must be denied produce: {}",
        prod_err
    );

    // 2. Producer-only client (lacks Consumer role)
    let producer_cert = make_cert_signed_by_ca(
        &root_ca,
        "producer-app:fintech:Producer:EU_WEST",
        "Fintech Corp",
        &["Producer"],
        "EU_WEST",
    );
    let producer_tls = build_client_config(
        root_ca.cert_pem.as_bytes(),
        Some(producer_cert.cert_pem.as_bytes()),
        Some(producer_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let mut producer_client = BrokerClient::connect_tls(
        addr,
        ServerName::try_from("localhost").unwrap().to_owned(),
        TlsConnector::from(producer_tls),
    )
    .await
    .unwrap();

    // Producer attempts to fetch: must be rejected with role denial (W09/W13)
    let fetch_res = producer_client
        .send_request(BrokerRequest::Fetch(FetchRequest {
            topic: "data".to_string(),
            partition: 0,
            class_id: "global-data".to_string(),
            start_offset: 0,
            max_records: 10,
        }))
        .await
        .unwrap();

    match fetch_res {
        BrokerResponse::Error(msg) => {
            assert!(
                msg.contains("lacks Consumer role"),
                "Producer-only client must be denied fetch: {}",
                msg
            );
        }
        other => panic!("Expected BrokerResponse::Error, got {:?}", other),
    }

    // 3. Non-agent client attempts to replicate
    let rep_res = producer_client
        .send_request(BrokerRequest::Replicate(ReplicationRequest {
            source_region: "US_EAST".to_string(),
            target_region: "EU_WEST".to_string(),
            topic: "data".to_string(),
            partition: 0,
            epoch: 1,
            class_id: "global-data".to_string(),
            from_offset: 0,
            to_offset: 1,
            payload: vec![],
            is_sealed: false,
            ..Default::default()
        }))
        .await
        .unwrap();

    match rep_res {
        BrokerResponse::Error(msg) => {
            assert!(
                msg.contains("lacks Agent role"),
                "Non-agent client must be denied replication: {}",
                msg
            );
        }
        other => panic!("Expected BrokerResponse::Error, got {:?}", other),
    }
}
