use clap::Parser;
use kamish_agent::ReplicationAgent;
use kamish_audit::AuditLog;
use kamish_broker::BrokerServer;
use kamish_enforce::Enforcer;
use kamish_meta::MetadataStore;
use kamish_policy::PolicyCache;
use kamish_route::RouteSelector;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "kamish-agent", about = "Kamish Inter-Region Replication Agent")]
struct Args {
    #[arg(short, long, default_value = "region-a")]
    local_region: String,

    #[arg(short, long)]
    target_region: Option<String>,

    #[arg(short, long, default_value = "./data/broker")]
    data_dir: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!(region = %args.local_region, target = ?args.target_region, "Starting Kamish replication agent");

    let audit_log = AuditLog::open(format!("{}/audit.log", args.data_dir))
        .unwrap_or_else(|_| AuditLog::new_in_memory());
    let policy_cache = Arc::new(RwLock::new(PolicyCache::new(HashSet::new())));
    let enforcer = Arc::new(Enforcer::new(&args.local_region, policy_cache, audit_log));
    let meta = Arc::new(MetadataStore::new());

    let broker = Arc::new(BrokerServer::new(
        &args.local_region,
        enforcer.clone(),
        meta.clone(),
        args.target_region.iter().cloned().collect(),
        &args.data_dir,
    ));

    let router = RouteSelector::new(&args.local_region, (*enforcer).clone());
    let _agent = ReplicationAgent::new(&args.local_region, broker, meta, router);

    info!(region = %args.local_region, "Kamish replication agent active and monitoring outbox streams.");
    Ok(())
}
