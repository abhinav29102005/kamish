use clap::Parser;
use kamish_audit::AuditLog;
use kamish_broker::{BrokerConfig, BrokerServer, build_server_config};
use kamish_enforce::Enforcer;
use kamish_meta::MetadataStore;
use kamish_policy::{PolicyCache, load_trusted_keys_from_file, load_verified_policy_from_file};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(name = "kamish-broker", about = "Kamish Regional Broker Daemon")]
struct Args {
    /// Path to a TOML configuration file
    #[arg(short, long)]
    config: Option<String>,

    #[arg(short, long, default_value = "region-a")]
    region: String,

    #[arg(short, long, default_value = "./data/broker")]
    data_dir: String,

    #[arg(short, long, default_value = "8080")]
    port: u16,

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, value_delimiter = ',')]
    peers: Vec<String>,

    /// Path to signed policy JSON file to verify and load at startup (W11)
    #[arg(long)]
    policy: Option<String>,

    /// Path to trusted public keys file (W11)
    #[arg(long)]
    trusted_keys: Option<String>,

    /// Minimum durable policy version floor (W11)
    #[arg(long)]
    policy_floor: Option<u64>,

    /// In-region replica node ID
    #[arg(long)]
    node_id: Option<String>,

    /// Acknowledgment durability level (LeaderFsync or InRegionQuorum)
    #[arg(long)]
    ack_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let mut config = if let Some(config_path) = args.config {
        info!(path = %config_path, "Loading broker configuration from file");
        BrokerConfig::from_file(&config_path)?
    } else {
        BrokerConfig {
            region: args.region,
            host: args.host,
            port: args.port,
            data_dir: args.data_dir,
            peers: args.peers,
            limits: Default::default(),
            metadata_endpoints: Vec::new(),
            tls: None,
            topics: Vec::new(),
            policy_path: args.policy.clone(),
            trusted_keys_path: args.trusted_keys.clone(),
            policy_version_floor: args.policy_floor,
            node_id: args.node_id.clone(),
            ack_level: args.ack_level.as_ref().map(|s| {
                if s.eq_ignore_ascii_case("InRegionQuorum") || s.eq_ignore_ascii_case("quorum") {
                    kamish_broker::AckLevel::InRegionQuorum
                } else {
                    kamish_broker::AckLevel::LeaderFsync
                }
            }),
            replica_group: None,
            kms: None,
        }
    };

    if let Some(ref s) = args.ack_level {
        config.ack_level = Some(
            if s.eq_ignore_ascii_case("InRegionQuorum") || s.eq_ignore_ascii_case("quorum") {
                kamish_broker::AckLevel::InRegionQuorum
            } else {
                kamish_broker::AckLevel::LeaderFsync
            },
        );
    }

    if args.policy.is_some() {
        config.policy_path = args.policy;
    }
    if args.trusted_keys.is_some() {
        config.trusted_keys_path = args.trusted_keys;
    }
    if args.policy_floor.is_some() {
        config.policy_version_floor = args.policy_floor;
    }

    info!(
        region = %config.region,
        host = %config.host,
        port = %config.port,
        peers = ?config.peers,
        limits = ?config.limits,
        "Starting Kamish broker daemon"
    );

    std::fs::create_dir_all(&config.data_dir)?;

    // 1. Durable policy version floor persistence (W11)
    let floor_file_path = std::path::Path::new(&config.data_dir).join("policy_floor.txt");
    let mut floor: u64 = if floor_file_path.exists() {
        let content = std::fs::read_to_string(&floor_file_path)?;
        content.trim().parse().unwrap_or(0)
    } else {
        0
    };
    if let Some(cfg_floor) = config.policy_version_floor {
        floor = std::cmp::max(floor, cfg_floor);
    }
    std::fs::write(&floor_file_path, floor.to_string())?;

    // 2. Load and verify trusted keys & policy at boot (W11)
    let trusted_keys = if let Some(ref keys_path) = config.trusted_keys_path {
        info!(path = %keys_path, "Loading trusted policy signing keys");
        load_trusted_keys_from_file(keys_path)?
    } else {
        HashSet::new()
    };

    let mut cache = PolicyCache::new(trusted_keys.clone()).with_version_floor(floor);

    let now_sec = chrono::Utc::now().timestamp();
    if let Some(ref policy_path) = config.policy_path {
        info!(path = %policy_path, floor = floor, "Verifying and loading startup sovereignty policy (W11)");
        let signed_policy =
            load_verified_policy_from_file(policy_path, &trusted_keys, floor, now_sec)?;
        let policy_ver = signed_policy.policy.version;
        cache.update(signed_policy, now_sec)?;
        if policy_ver > floor {
            floor = policy_ver;
            std::fs::write(&floor_file_path, floor.to_string())?;
        }
        info!(
            version = policy_ver,
            "Verified policy successfully installed into daemon cache"
        );
    }

    let policy_cache = Arc::new(RwLock::new(cache));
    let audit_log = AuditLog::open(format!("{}/audit.log", config.data_dir))?;
    let enforcer = Arc::new(Enforcer::new(&config.region, policy_cache, audit_log));
    let meta: Arc<dyn kamish_meta::MetadataAuthority> = if !config.metadata_endpoints.is_empty() {
        info!(
            endpoints = ?config.metadata_endpoints,
            "Connecting to external etcd 3.7.3 metadata cluster"
        );
        let ca_cert = config.tls.as_ref().map(|t| t.ca_cert_path.as_str());
        let client_cert = config.tls.as_ref().map(|t| t.cert_path.as_str());
        let client_key = config.tls.as_ref().map(|t| t.key_path.as_str());
        Arc::new(kamish_meta::EtcdMetadataAuthority::new(
            config.metadata_endpoints.clone(),
            ca_cert,
            client_cert,
            client_key,
            None,
        )?)
    } else {
        Arc::new(MetadataStore::new())
    };

    let catalog = config.build_catalog();
    let mut broker_builder = BrokerServer::try_new(
        &config.region,
        enforcer,
        meta,
        config.peers,
        &config.data_dir,
    )?
    .with_limits(config.limits)
    .with_catalog(catalog)
    .with_replica_group(config.replica_group.clone())
    .with_ack_level(
        config
            .ack_level
            .unwrap_or(kamish_broker::AckLevel::LeaderFsync),
    );

    if let Some(ref kms_cfg) = config.kms {
        info!(
            provider = %kms_cfg.provider_name,
            endpoint = %kms_cfg.endpoint_url,
            "Configuring external KMS provider boundary for sealed replication"
        );
        let provider = Arc::new(kamish_seal::ExternalKmsProvider::new(kms_cfg.clone()));
        broker_builder = broker_builder.with_key_provider(provider);
    }

    let broker = Arc::new(broker_builder);

    // Start proactive lease renewal worker conforming to CONTRACT.md §2.3.2 (10s TTL, 3s interval, 1s timeout)
    let renewal_task = broker.start_lease_renewal_worker(
        std::time::Duration::from_secs(3),
        std::time::Duration::from_secs(1),
        10_000,
    );

    // Start autonomous audit witness publisher worker (§5.1, W08)
    let (witness_shutdown_tx, witness_shutdown_rx) = tokio::sync::watch::channel(false);
    let witness_task = kamish_audit::spawn_witness_publisher_worker(
        broker.witness_publisher.clone(),
        witness_shutdown_rx,
        std::time::Duration::from_secs(5),
    );

    // Start automated safe segment retention worker (§4, Workstream D)
    let (retention_shutdown_tx, retention_shutdown_rx) = tokio::sync::watch::channel(false);
    let retention_task = kamish_broker::spawn_retention_worker(
        broker.retention.clone(),
        broker.clone(),
        retention_shutdown_rx,
        std::time::Duration::from_secs(60),
    );

    // Start consumer group session expiration sweeper worker (§5.1, Workstream C)
    let (cg_shutdown_tx, cg_shutdown_rx) = tokio::sync::watch::channel(false);
    let cg_task = kamish_broker::spawn_consumer_group_sweeper_worker(
        broker.consumer_groups.clone(),
        broker.clone(),
        cg_shutdown_rx,
        std::time::Duration::from_secs(5),
    );

    // Start in-region quorum heartbeat worker if replica group is configured
    let (quorum_shutdown_tx, quorum_shutdown_rx) = tokio::sync::watch::channel(false);
    let quorum_task = kamish_broker::spawn_quorum_heartbeat_worker(
        broker.quorum.clone(),
        broker.clone(),
        quorum_shutdown_rx,
    );

    let bind_addr = format!("{}:{}", config.host, config.port);
    let (addr, server_task) = if let Some(ref tls_cfg) = config.tls {
        info!(
            cert = %tls_cfg.cert_path,
            ca = %tls_cfg.ca_cert_path,
            require_client_auth = tls_cfg.require_client_auth,
            "Starting TLS 1.3 mTLS listener (W09)"
        );
        let cert_pem = std::fs::read(&tls_cfg.cert_path)?;
        let key_pem = std::fs::read(&tls_cfg.key_path)?;
        let ca_pem = std::fs::read(&tls_cfg.ca_cert_path)?;
        let server_tls_config = build_server_config(
            &cert_pem,
            &key_pem,
            Some(&ca_pem),
            tls_cfg.require_client_auth,
        )?;
        let acceptor = tokio_rustls::TlsAcceptor::from(server_tls_config);
        broker
            .clone()
            .start_tls_listener(&bind_addr, acceptor)
            .await?
    } else {
        broker.clone().start_tcp_listener(&bind_addr).await?
    };

    info!(
        region = %config.region,
        listen_addr = %addr,
        "Kamish broker active and serving requests."
    );

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, terminating broker.");
            broker.stop_lease_renewal();
            let _ = witness_shutdown_tx.send(true);
            let _ = renewal_task.await;
            let _ = retention_shutdown_tx.send(true);
            let _ = cg_shutdown_tx.send(true);
            let _ = witness_task.await;
            let _ = retention_task.await;
            let _ = cg_task.await;
            let _ = quorum_shutdown_tx.send(true);
            if let Some(task) = quorum_task {
                let _ = task.await;
            }
        }
        res = server_task => {
            if let Err(e) = res {
                error!(error = %e, "Broker listener task aborted");
            }
            broker.stop_lease_renewal();
            let _ = witness_shutdown_tx.send(true);
            let _ = renewal_task.await;
            let _ = retention_shutdown_tx.send(true);
            let _ = cg_shutdown_tx.send(true);
            let _ = witness_task.await;
            let _ = retention_task.await;
            let _ = cg_task.await;
            let _ = quorum_shutdown_tx.send(true);
            if let Some(task) = quorum_task {
                let _ = task.await;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_arguments_are_valid() {
        Args::command().debug_assert();
        let error = Args::try_parse_from(["kamish-broker", "--help"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
