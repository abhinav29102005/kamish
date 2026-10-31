use clap::{Parser, Subcommand, ValueEnum};
use kamish_audit::AuditLog;
use kamish_client::{KamishAdminClient, KamishConsumer, KamishProducer};
use kamish_meta::{KamishPolicyEngine, RegionId, ResidencyPolicy};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "kamish-ctl", about = "Kamish Administrative, Policy, and Demonstration Control Tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Debug, ValueEnum)]
enum PolicyChoice {
    StrictLocal,
    GlobalCleartext,
    RestrictedEuUs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Verify the cryptographic integrity of a hash-chained audit log (FR-13)
    VerifyAudit {
        #[arg(short, long)]
        log_path: String,
    },

    /// Produce an event to a running Kamish broker instance with client-side policy validation
    Produce {
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,
        #[arg(short, long, default_value = "telemetry")]
        topic: String,
        #[arg(short, long, default_value_t = 0)]
        partition: u32,
        #[arg(short, long, default_value = "general")]
        class_id: String,
        #[arg(short, long)]
        payload: String,
        #[arg(short, long, default_value = "EU_WEST")]
        region: String,
        #[arg(short, long, value_enum, default_value = "strict-local")]
        policy: PolicyChoice,
    },

    /// Sequentially consume records from a Kamish broker partition up to HWM (INV-4)
    Consume {
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,
        #[arg(short, long, default_value = "telemetry")]
        topic: String,
        #[arg(short, long, default_value_t = 0)]
        partition: u32,
        #[arg(short, long, default_value = "general")]
        class_id: String,
        #[arg(short, long, default_value_t = 0)]
        offset: u64,
        #[arg(short, long, default_value_t = 10)]
        max_records: usize,
    },

    /// Query the durable replication watermark from a remote broker (INV-6)
    Watermark {
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,
        #[arg(short, long, default_value = "telemetry")]
        topic: String,
        #[arg(short, long, default_value_t = 0)]
        partition: u32,
        #[arg(short, long, default_value = "EU_WEST")]
        source_region: String,
    },

    /// Run the Developer B 3-Scenario verification suite (Residency, Slow-Link, Outage)
    Demo,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::VerifyAudit { log_path } => {
            println!("\x1b[1;34m[kamish-ctl]\x1b[0m Inspecting cryptographic audit log at: {}", log_path);
            let log = AuditLog::open(&log_path)?;
            let count = log.verify_integrity()?;
            println!("\x1b[1;32m[kamish-ctl]\x1b[0m  Audit log integrity verified: {} entries unbroken in cryptographic chain.", count);
        }

        Commands::Produce {
            addr,
            topic,
            partition,
            class_id,
            payload,
            region,
            policy,
        } => {
            let region_id = match region.to_uppercase().as_str() {
                "EU_WEST" => RegionId::EuWest,
                "US_EAST" => RegionId::UsEast,
                "AP_SOUTH" => RegionId::ApSouth,
                custom => RegionId::Custom(custom.to_string()),
            };

            let residency = match policy {
                PolicyChoice::StrictLocal => ResidencyPolicy::StrictLocal,
                PolicyChoice::GlobalCleartext => ResidencyPolicy::GlobalCleartext,
                PolicyChoice::RestrictedEuUs => {
                    ResidencyPolicy::RestrictedReplication(vec![RegionId::EuWest, RegionId::UsEast])
                }
            };

            let engine = Arc::new(KamishPolicyEngine::new(AuditLog::new_in_memory()));
            println!(
                "\x1b[1;34m[kamish-ctl]\x1b[0m Connecting producer to broker at {} (Region: {:?}, Policy: {:?})...",
                addr, region_id, residency
            );

            let producer = KamishProducer::connect(&addr, 1001, region_id.clone(), Some(engine.clone())).await?;
            let ack = producer
                .send(&topic, partition, &class_id, payload.into_bytes(), residency)
                .await?;

            println!(
                "\x1b[1;32m[kamish-ctl]\x1b[0m  Record successfully acknowledged! Base Offset: {}, Committed Epoch: {}",
                ack.base_offset, ack.committed_epoch
            );
        }

        Commands::Consume {
            addr,
            topic,
            partition,
            class_id,
            offset,
            max_records,
        } => {
            println!(
                "\x1b[1;34m[kamish-ctl]\x1b[0m Connecting consumer to {} (Topic: {}, Partition: {}, Start Offset: {})...",
                addr, topic, partition, offset
            );

            let mut consumer = KamishConsumer::connect(&addr, offset).await?;
            let records = consumer.fetch(&topic, partition, &class_id, max_records).await?;

            println!("\x1b[1;32m[kamish-ctl]\x1b[0m Fetched {} records:", records.len());
            for record in records {
                let payload_str = String::from_utf8_lossy(&record.payload);
                println!(
                    "  [Offset {:04}] Epoch: {} | Class: {} | Payload: {}",
                    record.base_offset, record.epoch, record.class_id, payload_str
                );
            }
        }

        Commands::Watermark {
            addr,
            topic,
            partition,
            source_region,
        } => {
            println!(
                "\x1b[1;34m[kamish-ctl]\x1b[0m Querying durable watermark from {} (Source Region: {})...",
                addr, source_region
            );
            let admin = KamishAdminClient::connect(&addr).await?;
            let watermark = admin.get_watermark(&topic, partition, &source_region).await?;
            println!(
                "\x1b[1;32m[kamish-ctl]\x1b[0m  Durable Replication Watermark for {}:{}: {}",
                topic, partition, watermark
            );
        }

        Commands::Demo => {
            println!("\x1b[1;35m========================================================================\x1b[0m");
            println!("\x1b[1;36m  KAMISH BROKER — DEVELOPER B LIVE DEMO SCENARIOS RUNNER (CLI)\x1b[0m");
            println!("\x1b[1;35m========================================================================\x1b[0m");

            let status = std::process::Command::new("bash")
                .arg("scripts/run_demo.sh")
                .status()?;

            if !status.success() {
                eprintln!("\x1b[1;31m[kamish-ctl] Demo suite encountered an error.\x1b[0m");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
