use clap::{Parser, Subcommand};
use kamish_audit::AuditLog;

#[derive(Parser, Debug)]
#[command(name = "kamish-ctl", about = "Kamish Administrative and Policy Control Tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Verify the integrity of a hash-chained audit log
    VerifyAudit {
        #[arg(short, long)]
        log_path: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::VerifyAudit { log_path } => {
            println!("Opening audit log at {}", log_path);
            let log = AuditLog::open(&log_path)?;
            let count = log.verify_integrity()?;
            println!(" Audit log integrity verified: {} entries unbroken in cryptographic chain.", count);
        }
    }
    Ok(())
}
