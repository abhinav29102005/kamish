//! Loopback-only integration fixture for packages/kamish-client.
//! Synthetic signing key and policy: NEVER use this example as a production server.
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_broker::BrokerServer;
use kamish_enforce::Enforcer;
use kamish_meta::MetadataStore;
use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::args().nth(1).ok_or("Provide an isolated test data directory")?;
    let key = SigningKey::from_bytes(&[11; 32]);
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let region = "EU_WEST";
    let class = ClassRule {
        class_id: "general".into(),
        description: "Synthetic SDK integration data".into(),
        legal_regions: HashSet::from([region.into()]),
        destination_modes: HashMap::from([(region.into(), DestinationMode::Cleartext)]),
        ack_mode: AckMode::LocalMajority,
        failover_candidates: vec![region.into()],
        transit_allowed: false,
    };
    let policy = PolicyDocument {
        policy_id: "sdk-test-only".into(),
        version: 1,
        issued_at_sec: chrono::Utc::now().timestamp() - 1,
        ttl_seconds: 3600,
        issuer_pubkey_hex: public_key.clone(),
        classes: HashMap::from([("general".into(), class)]),
        transit_rules: HashMap::new(),
    };
    let mut cache = PolicyCache::new(HashSet::from([public_key]));
    cache.update(SignedPolicy::sign(policy, &key), chrono::Utc::now().timestamp())?;
    let enforcer = Arc::new(Enforcer::new(
        region, Arc::new(RwLock::new(cache)), AuditLog::new_in_memory(),
    ));
    let broker = Arc::new(BrokerServer::try_new(
        region, enforcer, Arc::new(MetadataStore::new()), vec![], data_dir,
    )?);
    let (addr, task) = broker.start_tcp_listener("127.0.0.1:0").await?;
    println!("{}", serde_json::json!({ "host": "127.0.0.1", "port": addr.port() }));
    tokio::signal::ctrl_c().await?;
    task.abort();
    Ok(())
}
