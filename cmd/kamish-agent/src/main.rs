use clap::Parser;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "kamish-agent", about = "Kamish Inter-Region Replication Agent")]
struct Args {
    #[arg(short, long, default_value = "region-a")]
    local_region: String,

    #[arg(short, long)]
    target_region: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!(region = %args.local_region, "Starting Kamish replication agent");
    Ok(())
}
