use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::tls::test_helpers::*;
use kamish_broker::{
    BrokerClient, BrokerRequest, BrokerResponse, BrokerServer, FetchRequest, ProduceRequest,
    TenantCatalog, TopicMetadata, build_client_config, build_server_config,
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
use tokio_rustls::{TlsAcceptor, TlsConnector};

fn setup_test_broker(
    temp_dir: &tempfile::TempDir,
    catalog: TenantCatalog,
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
        "fintech-transfer".to_string(),
        ClassRule {
            class_id: "fintech-transfer".to_string(),
            description: "Fintech financial transaction data".to_string(),
            legal_regions: legal.clone(),
            destination_modes: modes.clone(),
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["EU_WEST".to_string()],
            transit_allowed: true,
        },
    );
    classes.insert(
        "medical-records".to_string(),
        ClassRule {
            class_id: "medical-records".to_string(),
            description: "Health records class for healthcare tenant".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["EU_WEST".to_string()],
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
        .unwrap()
        .with_catalog(catalog),
    );

    (broker, meta, enforcer)
}

#[tokio::test]
async fn test_tenant_catalog_and_fetch_isolation() {
    let temp_dir = tempfile::tempdir().unwrap();

    // 1. Configure TenantCatalog (W10 / W13)
    let mut catalog = TenantCatalog::new();
    let payments_meta = TopicMetadata::new(
        "payments",
        "fintech",
        1,
        HashSet::from(["fintech-transfer".to_string()]),
    )
    .with_consumer_regions(HashSet::from(["EU_WEST".to_string()]))
    .with_producers(HashSet::from(["fintech-producer-1".to_string()]));

    let health_meta = TopicMetadata::new(
        "health",
        "healthcorp",
        1,
        HashSet::from(["medical-records".to_string()]),
    )
    .with_consumer_regions(HashSet::from(["US_EAST".to_string()]));

    catalog.register_topic(payments_meta);
    catalog.register_topic(health_meta);

    let (broker, _meta, _enf) = setup_test_broker(&temp_dir, catalog);

    // 2. Generate Root CA and certificates
    let root_ca = make_ca("Kamish Root CA");
    let server_cert = make_cert_signed_by_ca(
        &root_ca,
        "localhost",
        "Kamish Infrastructure",
        &["Broker"],
        "EU_WEST",
    );

    // Client identities:
    // A. Authorized fintech producer in EU_WEST
    let fintech_prod_cert = make_cert_signed_by_ca(
        &root_ca,
        "fintech-producer-1:fintech:Producer:EU_WEST",
        "Fintech Corp",
        &["Producer"],
        "EU_WEST",
    );

    // B. Healthcorp producer trying to produce to fintech topic
    let health_prod_cert = make_cert_signed_by_ca(
        &root_ca,
        "health-producer-1:healthcorp:Producer:EU_WEST",
        "Health Corp",
        &["Producer"],
        "EU_WEST",
    );

    // C. Authorized fintech consumer in EU_WEST
    let fintech_cons_eu_cert = make_cert_signed_by_ca(
        &root_ca,
        "fintech-consumer-eu:fintech:Consumer:EU_WEST",
        "Fintech Corp",
        &["Consumer"],
        "EU_WEST",
    );

    // D. Fintech consumer in US_EAST (forbidden region for payments topic)
    let fintech_cons_us_cert = make_cert_signed_by_ca(
        &root_ca,
        "fintech-consumer-us:fintech:Consumer:US_EAST",
        "Fintech Corp",
        &["Consumer"],
        "US_EAST",
    );

    // Start TLS 1.3 mTLS Listener
    let server_tls = build_server_config(
        server_cert.cert_pem.as_bytes(),
        server_cert.key_pem.as_bytes(),
        Some(root_ca.cert_pem.as_bytes()),
        true,
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(server_tls);
    let (addr, _task) = broker
        .start_tls_listener("127.0.0.1:0", acceptor)
        .await
        .unwrap();

    // --- Scenario 1: Authorized fintech producer produces with authorized class ---
    let client_tls = build_client_config(
        root_ca.cert_pem.as_bytes(),
        Some(fintech_prod_cert.cert_pem.as_bytes()),
        Some(fintech_prod_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let connector = TlsConnector::from(client_tls);
    let server_name = ServerName::try_from("localhost").unwrap().to_owned();
    let mut prod_client = BrokerClient::connect_tls(addr, server_name, connector)
        .await
        .unwrap();

    let produce_ok = ProduceRequest {
        topic: "payments".to_string(),
        partition: 0,
        producer_id: 101,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "fintech-transfer".to_string(),
        payload: b"transfer-valid-$1000".to_vec(),
        residency_rule: None,
    };
    let resp = prod_client
        .send_request(BrokerRequest::Produce(produce_ok))
        .await
        .unwrap();
    match resp {
        BrokerResponse::Produce(ack) => {
            assert_eq!(ack.base_offset, 0);
            assert!(ack.sovereignty_allowed);
        }
        other => panic!("Expected successful produce ACK, got: {:?}", other),
    }

    // --- Scenario 2 (W10): Producer cannot choose another tenant's class ---
    let produce_wrong_class = ProduceRequest {
        topic: "payments".to_string(),
        partition: 0,
        producer_id: 101,
        producer_epoch: 1,
        sequence_base: 1,
        class_id: "medical-records".to_string(), // Belongs to healthcorp, not allowed on payments!
        payload: b"malicious-class-injection".to_vec(),
        residency_rule: None,
    };
    let resp = prod_client
        .send_request(BrokerRequest::Produce(produce_wrong_class))
        .await
        .unwrap();
    match resp {
        BrokerResponse::Error(msg) => {
            assert!(
                msg.contains("Data class denied"),
                "Expected data class rejection, got: {}",
                msg
            );
            assert!(msg.contains("not authorized for topic 'payments'"));
        }
        other => panic!(
            "Expected error for unauthorized class selection (W10), got: {:?}",
            other
        ),
    }

    // --- Scenario 3 (W10): Healthcorp producer attempts to produce to fintech's topic ---
    let health_tls = build_client_config(
        root_ca.cert_pem.as_bytes(),
        Some(health_prod_cert.cert_pem.as_bytes()),
        Some(health_prod_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let health_conn = TlsConnector::from(health_tls);
    let server_name = ServerName::try_from("localhost").unwrap().to_owned();
    let mut health_client = BrokerClient::connect_tls(addr, server_name, health_conn)
        .await
        .unwrap();

    let produce_cross_tenant = ProduceRequest {
        topic: "payments".to_string(),
        partition: 0,
        producer_id: 202,
        producer_epoch: 1,
        sequence_base: 0,
        class_id: "fintech-transfer".to_string(),
        payload: b"cross-tenant-attack".to_vec(),
        residency_rule: None,
    };
    let resp = health_client
        .send_request(BrokerRequest::Produce(produce_cross_tenant))
        .await
        .unwrap();
    match resp {
        BrokerResponse::Error(msg) => {
            assert!(
                msg.contains("Tenant mismatch"),
                "Expected tenant mismatch rejection, got: {}",
                msg
            );
        }
        other => panic!(
            "Expected error for cross-tenant production (W10), got: {:?}",
            other
        ),
    }

    // --- Scenario 4 (W13): Authorized consumer in EU_WEST fetches successfully ---
    let eu_cons_tls = build_client_config(
        root_ca.cert_pem.as_bytes(),
        Some(fintech_cons_eu_cert.cert_pem.as_bytes()),
        Some(fintech_cons_eu_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let eu_conn = TlsConnector::from(eu_cons_tls);
    let server_name = ServerName::try_from("localhost").unwrap().to_owned();
    let mut eu_cons_client = BrokerClient::connect_tls(addr, server_name, eu_conn)
        .await
        .unwrap();

    let fetch_valid = FetchRequest {
        topic: "payments".to_string(),
        partition: 0,
        class_id: "fintech-transfer".to_string(),
        start_offset: 0,
        max_records: 10,
    };
    let resp = eu_cons_client
        .send_request(BrokerRequest::Fetch(fetch_valid))
        .await
        .unwrap();
    match resp {
        BrokerResponse::Fetch(res) => {
            assert_eq!(res.records.len(), 1);
            assert_eq!(res.records[0].payload, b"transfer-valid-$1000");
        }
        other => panic!("Expected successful fetch, got: {:?}", other),
    }

    // --- Scenario 5 (W13): Consumer in US_EAST attempts to fetch EU_WEST-restricted payments topic ---
    let us_cons_tls = build_client_config(
        root_ca.cert_pem.as_bytes(),
        Some(fintech_cons_us_cert.cert_pem.as_bytes()),
        Some(fintech_cons_us_cert.key_pem.as_bytes()),
    )
    .unwrap();
    let us_conn = TlsConnector::from(us_cons_tls);
    let server_name = ServerName::try_from("localhost").unwrap().to_owned();
    let mut us_cons_client = BrokerClient::connect_tls(addr, server_name, us_conn)
        .await
        .unwrap();

    let fetch_wrong_region = FetchRequest {
        topic: "payments".to_string(),
        partition: 0,
        class_id: "fintech-transfer".to_string(),
        start_offset: 0,
        max_records: 10,
    };
    let resp = us_cons_client
        .send_request(BrokerRequest::Fetch(fetch_wrong_region))
        .await
        .unwrap();
    match resp {
        BrokerResponse::Error(msg) => {
            assert!(
                msg.contains("Region boundary denied"),
                "Expected region boundary rejection, got: {}",
                msg
            );
            assert!(msg.contains("consumer in region 'US_EAST' cannot fetch topic 'payments'"));
        }
        other => panic!(
            "Expected error for consumer region boundary violation (W13), got: {:?}",
            other
        ),
    }
}
