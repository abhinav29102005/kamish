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
use std::time::Duration;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(name = "kamish-agent", about = "Kamish Inter-Region Replication Agent")]
struct Args {
    #[arg(short, long, default_value = "region-a")]
    local_region: String,

    #[arg(short, long)]
    target_region: Option<String>,

    #[arg(long)]
    target_addr: Option<String>,

    #[arg(short, long, default_value = "./data/broker")]
    data_dir: String,

    #[arg(long, default_value = "500")]
    poll_interval_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!(
        region = %args.local_region,
        target_region = ?args.target_region,
        target_addr = ?args.target_addr,
        "Starting Kamish replication agent"
    );

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
    let agent = Arc::new(ReplicationAgent::new(
        &args.local_region,
        broker,
        meta,
        router,
    ));

    if let (Some(target), Some(addr)) = (&args.target_region, &args.target_addr) {
        agent.register_remote_peer(target, addr);
        info!(target = %target, addr = %addr, "Registered remote broker TCP endpoint for replication");

        let target_str = target.clone();
        let agent_task = Arc::clone(&agent);
        let interval = Duration::from_millis(args.poll_interval_ms);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Err(e) = agent_task.step_replication_tcp(&target_str).await {
                    error!(target = %target_str, error = %e, "Replication step failed");
                }
            }
        });
    }

    info!(region = %args.local_region, "Kamish replication agent active and monitoring outbox streams.");

    tokio::signal::ctrl_c().await?;
    info!("Replication agent shutting down.");
    Ok(())
}
