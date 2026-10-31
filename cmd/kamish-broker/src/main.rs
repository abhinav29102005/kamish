use clap::Parser;
use kamish_audit::AuditLog;
use kamish_broker::BrokerServer;
use kamish_enforce::Enforcer;
use kamish_meta::MetadataStore;
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

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, value_delimiter = ',')]
    peers: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!(region = %args.region, host = %args.host, port = %args.port, peers = ?args.peers, "Starting Kamish broker");

    std::fs::create_dir_all(&args.data_dir)?;
    let audit_log = AuditLog::open(format!("{}/audit.log", args.data_dir))?;
    let policy_cache = Arc::new(RwLock::new(PolicyCache::new(HashSet::new())));
    let enforcer = Arc::new(Enforcer::new(&args.region, policy_cache, audit_log));
    let meta = Arc::new(MetadataStore::new());

    let broker = Arc::new(BrokerServer::try_new(
        &args.region,
        enforcer,
        meta,
        args.peers,
        &args.data_dir,
    )?);

    let bind_addr = format!("{}:{}", args.host, args.port);
    let (addr, server_task) = broker.start_tcp_listener(&bind_addr).await?;

    info!(
        region = %args.region,
        listen_addr = %addr,
        "Kamish broker async framed TCP listener active and serving requests."
    );

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, terminating broker.");
        }
        res = server_task => {
            if let Err(e) = res {
                tracing::error!(error = %e, "Broker listener task aborted");
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
