//! Kamish Dashboard Gateway Daemon Entry Point (Phase 5).

use clap::Parser;
use kamish_broker::BrokerServer;
use kamish_broker::catalog::{TenantCatalog, TopicMetadata};
use kamish_enforce::Enforcer;
use kamish_gateway::{GatewayState, app};
use kamish_meta::MetadataStore;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(author, version, about = "Kamish Dashboard Gateway Service")]
struct Args {
    /// Port for HTTP REST and SSE Gateway
    #[arg(short, long, default_value_t = 3001)]
    port: u16,

    /// Region identifier for the broker
    #[arg(short, long, default_value = "EU_WEST")]
    region: String,

    /// Data directory for partition logs and persistent checkpoints
    #[arg(short, long, default_value = "data/gateway-broker")]
    data_dir: PathBuf,

    /// Known peer regions (comma-separated)
    #[arg(long, default_value = "US_EAST,AP_SOUTH")]
    peers: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kamish_gateway=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let peers: Vec<String> = args
        .peers
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    std::fs::create_dir_all(&args.data_dir)?;

    let audit_log = kamish_audit::AuditLog::open(args.data_dir.join("audit.log"))?;
    let (policy_cache, _signing_key) =
        kamish_gateway::create_default_policy_cache(&["EU_WEST", "US_EAST", "AP_SOUTH"]);
    let enforcer = Arc::new(Enforcer::new(&args.region, policy_cache, audit_log));
    let meta = Arc::new(MetadataStore::new());

    let mut catalog = TenantCatalog::new();
    let mut sovereign_classes = HashSet::new();
    sovereign_classes.insert("sovereign-class".to_string());
    catalog.register_topic(TopicMetadata::new(
        "financial-transactions",
        "tenant-sovereign",
        3,
        sovereign_classes.clone(),
    ));
    catalog.register_topic(TopicMetadata::new(
        "sovereign-stream",
        "tenant-sovereign",
        3,
        sovereign_classes,
    ));

    let mut general_classes = HashSet::new();
    general_classes.insert("general".to_string());
    catalog.register_topic(TopicMetadata::new(
        "general-topic",
        "tenant-default",
        3,
        general_classes,
    ));

    let broker = Arc::new(
        BrokerServer::try_new(args.region.clone(), enforcer, meta, peers, &args.data_dir)?
            .with_catalog(catalog),
    );

    // Register active initial partitions
    for p in 0..3 {
        let _ = broker.claim_partition_ownership("financial-transactions", p, 0, 1, 30_000);
        let _ = broker.claim_partition_ownership("sovereign-stream", p, 0, 1, 30_000);
        let _ = broker.claim_partition_ownership("general-topic", p, 0, 1, 30_000);
    }

    let state = GatewayState::new(broker);
    let app = app(state);

    let bind_addr = format!("0.0.0.0:{}", args.port);
    let listener = TcpListener::bind(&bind_addr).await?;
    tracing::info!(
        region = %args.region,
        port = args.port,
        "Kamish Dashboard Gateway listening on http://{}",
        bind_addr
    );

    axum::serve(listener, app).await?;
    Ok(())
}
