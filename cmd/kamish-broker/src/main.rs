use clap::Parser;
use kamish_audit::AuditLog;
use kamish_enforce::Enforcer;
use kamish_policy::PolicyCache;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "kamish-broker", about = "Kamish Regional Broker Daemon")]
struct Args {
    #[arg(short, long, default_value = "region-a")]
    region: String,

    #[arg(short, long, default_value = "./data/broker")]
    data_dir: String,

    #[arg(short, long, default_value = "8080")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!(region = %args.region, port = %args.port, "Starting Kamish broker");

    let audit_log = AuditLog::open(format!("{}/audit.log", args.data_dir))
        .unwrap_or_else(|_| AuditLog::new_in_memory());
    let policy_cache = Arc::new(RwLock::new(PolicyCache::new(HashSet::new())));
    let _enforcer = Enforcer::new(&args.region, policy_cache, audit_log);

    info!("Kamish broker initialized successfully with zero-payload audit logging.");
    Ok(())
}
