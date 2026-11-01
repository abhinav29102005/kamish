pub mod assets;
pub mod completion;
pub mod ctl;
pub mod doctor;
pub mod init;
pub mod policy_cmd;
pub mod seed;
pub mod start;
pub mod status;
pub mod topic;

use clap::{CommandFactory, Parser, Subcommand};
use ctl::PolicyChoice;

#[derive(Parser, Debug)]
#[command(
    name = "kamish",
    version = env!("CARGO_PKG_VERSION"),
    about = "Kamish: Sovereign Event Streaming Broker, Gateway & Management CLI",
    long_about = "Kamish delivers verifiable data sovereignty across hybrid multi-region topologies\nwith continuous cryptographic enforcement and zero unsolicited telemetry."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the sovereign broker, gateway API, and embedded dashboard in one process
    Start {
        /// Path to TOML configuration file
        #[arg(short, long)]
        config: Option<String>,

        /// Gateway HTTP and Dashboard port
        #[arg(short, long, default_value_t = 3001)]
        port: u16,

        /// Broker TCP messaging port
        #[arg(short = 'b', long, default_value_t = 9092)]
        broker_port: u16,

        /// Data directory for partition logs and audit chains
        #[arg(short, long, default_value = "./data/kamish")]
        data_dir: String,

        /// Local region identifier
        #[arg(short, long, default_value = "EU_WEST")]
        region: String,

        /// Interface address to bind
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Enable development mode credentials
        #[arg(long, default_value_t = false)]
        dev: bool,

        /// Explicit opt-in for binding non-loopback interface without TLS
        #[arg(long, default_value_t = false)]
        insecure_dev: bool,
    },

    /// Run comprehensive prerequisite diagnostics (versions, ports, consensus, certificates)
    Doctor {
        /// Optional path to kamish.toml to validate
        #[arg(short, long)]
        config: Option<String>,

        /// Output diagnostic report as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// First-run setup initializing configuration skeletons and administrator credentials
    Init {
        /// Target directory to initialize
        #[arg(short, long, default_value = ".")]
        dir: String,

        /// Path to secret file containing admin password
        #[arg(short, long)]
        secret_file: Option<String>,

        /// Administrator username to provision
        #[arg(long, default_value = "admin")]
        admin_user: String,

        /// Direct admin password (for automated non-interactive scripting)
        #[arg(long)]
        admin_password: Option<String>,

        /// Primary cluster sovereign region
        #[arg(short, long, default_value = "EU_WEST")]
        region: String,

        /// Force overwrite of existing configuration files
        #[arg(short, long, default_value_t = false)]
        force: bool,
    },

    /// Query running cluster status, operating mode, and consensus authority model
    Status {
        /// Target Gateway URL
        #[arg(long, default_value = "http://127.0.0.1:3001")]
        gateway_url: String,

        /// Session token (optional, auto-authenticates if omitted)
        #[arg(long)]
        token: Option<String>,

        /// Admin username for authentication
        #[arg(long)]
        username: Option<String>,

        /// Admin password for authentication
        #[arg(long)]
        password: Option<String>,

        /// Emit machine-readable JSON output
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Sovereign topic creation and inspection
    Topic {
        #[command(subcommand)]
        cmd: TopicCommands,
    },

    /// Produce a record with cryptographic residency enforcement
    Produce {
        /// Target broker TCP address
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,

        /// Target topic name
        #[arg(short, long)]
        topic: String,

        /// Target partition number
        #[arg(short = 'p', long, default_value_t = 0)]
        partition: u32,

        /// Classification rule ID
        #[arg(short, long, default_value = "general")]
        class_id: String,

        /// Raw message payload bytes
        #[arg(long)]
        payload: String,

        /// Client home region
        #[arg(short, long, default_value = "EU_WEST")]
        region: String,

        /// Residency policy enforcement choice
        #[arg(value_enum, long, default_value = "strict-local")]
        policy: PolicyChoice,
    },

    /// Consume records from a partition beginning at an offset
    Consume {
        /// Target broker TCP address
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,

        /// Target topic name
        #[arg(short, long)]
        topic: String,

        /// Target partition number
        #[arg(short = 'p', long, default_value_t = 0)]
        partition: u32,

        /// Client classification identity
        #[arg(short, long, default_value = "general")]
        class_id: String,

        /// Offset to begin reading
        #[arg(short, long, default_value_t = 0)]
        offset: u64,

        /// Maximum number of records to fetch
        #[arg(short, long, default_value_t = 10)]
        max_records: usize,
    },

    /// Fetch records from a partition (alias for consume)
    Fetch {
        /// Target broker TCP address
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,

        /// Target topic name
        #[arg(short, long)]
        topic: String,

        /// Target partition number
        #[arg(short = 'p', long, default_value_t = 0)]
        partition: u32,

        /// Client classification identity
        #[arg(short, long, default_value = "general")]
        class_id: String,

        /// Offset to begin reading
        #[arg(short, long, default_value_t = 0)]
        offset: u64,

        /// Maximum number of records to fetch
        #[arg(short, long, default_value_t = 10)]
        max_records: usize,
    },

    /// Sovereign policy authoring, signature verification, and cluster publication
    Policy {
        #[command(subcommand)]
        cmd: PolicyCommands,
    },

    /// Verify the cryptographic integrity of a hash-chained audit log
    VerifyAudit {
        /// Path to the audit log file
        #[arg(short, long)]
        log_path: String,

        /// Optional stream ID to verify external witness anchoring
        #[arg(short, long)]
        stream: Option<String>,

        /// Optional external witness public key hex
        #[arg(short, long)]
        pubkey: Option<String>,

        /// Machine-readable JSON output
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Run real-time performance and latency benchmark across network conditions
    Benchmark {
        /// Total messages to produce
        #[arg(short, long, default_value_t = 100)]
        count: usize,

        /// Producer concurrency level
        #[arg(short = 'm', long, default_value_t = 2)]
        concurrency: usize,
    },

    /// Execute labelled scenario demonstration walkthrough
    Demo,

    /// Request partition failover promotion to a candidate owner
    Promote {
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,

        #[arg(short, long)]
        topic: String,

        #[arg(short = 'p', long, default_value_t = 0)]
        partition: u32,

        #[arg(short, long, default_value = "general")]
        class_id: String,

        #[arg(long, default_value_t = 0)]
        min_watermark: u64,
    },

    /// Query durable replication watermark for a source region
    Watermark {
        #[arg(short, long, default_value = "127.0.0.1:9092")]
        addr: String,

        #[arg(short, long)]
        topic: String,

        #[arg(short = 'p', long, default_value_t = 0)]
        partition: u32,

        #[arg(short, long, default_value = "EU_WEST")]
        source_region: String,
    },

    /// Generate shell auto-completion scripts
    Completion {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
enum TopicCommands {
    /// Create a new sovereign topic
    Create {
        /// Topic identifier
        #[arg(short, long)]
        name: String,

        /// Tenant identifier owning the topic
        #[arg(short, long, default_value = "tenant-sovereign")]
        tenant: String,

        /// Number of sovereign partitions
        #[arg(short = 'p', long, default_value_t = 3)]
        partitions: usize,

        /// Authorized classification classes
        #[arg(
            short,
            long,
            value_delimiter = ',',
            default_value = "sovereign-class,general"
        )]
        classes: Vec<String>,

        /// Target Gateway URL
        #[arg(long, default_value = "http://127.0.0.1:3001")]
        gateway_url: String,

        /// Pre-authenticated bearer token
        #[arg(long)]
        token: Option<String>,

        /// Username for credentials authentication
        #[arg(short, long)]
        username: Option<String>,

        /// Password for credentials authentication
        #[arg(long)]
        password: Option<String>,
    },

    /// List all registered sovereign topics
    List {
        /// Target Gateway URL
        #[arg(long, default_value = "http://127.0.0.1:3001")]
        gateway_url: String,

        /// Pre-authenticated bearer token
        #[arg(long)]
        token: Option<String>,

        /// Username for credentials authentication
        #[arg(short, long)]
        username: Option<String>,

        /// Password for credentials authentication
        #[arg(long)]
        password: Option<String>,

        /// Emit machine-readable JSON output
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum PolicyCommands {
    /// Sign an authoring policy JSON with an Ed25519 private key
    Sign {
        /// Path to policy document JSON
        #[arg(short, long)]
        policy: String,

        /// Path to Ed25519 private key file (hex seed)
        #[arg(short, long)]
        key: String,

        /// Optional output file for signed policy
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Verify a signed policy against a trusted public keys file
    Verify {
        /// Path to signed policy JSON
        #[arg(short, long)]
        policy: String,

        /// Path to trusted public keys file
        #[arg(short, long)]
        trusted_keys: String,
    },

    /// Publish and activate a signed policy on a running gateway cluster
    Publish {
        /// Path to signed policy JSON
        #[arg(short, long)]
        policy: String,

        /// Target Gateway URL
        #[arg(long, default_value = "http://127.0.0.1:3001")]
        gateway_url: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Start {
            config,
            port,
            broker_port,
            data_dir,
            region,
            host,
            dev,
            insecure_dev,
        } => {
            let opts = start::StartOptions {
                config: config.as_deref(),
                port,
                broker_port,
                data_dir: &data_dir,
                region: &region,
                host: &host,
                dev,
                insecure_dev,
            };
            match start::run_start(opts).await {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("Fatal error in kamish start: {e}");
                    1
                }
            }
        }

        Commands::Doctor { config, json } => doctor::run_doctor(config.as_deref(), json),

        Commands::Init {
            dir,
            secret_file,
            admin_user,
            admin_password,
            region,
            force,
        } => {
            let opts = init::InitOptions {
                dir: &dir,
                secret_file: secret_file.as_deref(),
                admin_user: &admin_user,
                admin_password: admin_password.as_deref(),
                region: &region,
                force,
            };
            init::run_init(opts)
        }

        Commands::Status {
            gateway_url,
            token,
            username,
            password,
            json,
        } => {
            let auth = status::AuthOptions {
                token: token.as_deref(),
                username: username.as_deref(),
                password: password.as_deref(),
            };
            status::run_status(&gateway_url, auth, json).await
        }

        Commands::Topic { cmd } => match cmd {
            TopicCommands::Create {
                name,
                tenant,
                partitions,
                classes,
                gateway_url,
                token,
                username,
                password,
            } => {
                let auth = status::AuthOptions {
                    token: token.as_deref(),
                    username: username.as_deref(),
                    password: password.as_deref(),
                };
                topic::create_topic(&gateway_url, &name, &tenant, partitions, &classes, auth).await
            }

            TopicCommands::List {
                gateway_url,
                token,
                username,
                password,
                json,
            } => {
                let auth = status::AuthOptions {
                    token: token.as_deref(),
                    username: username.as_deref(),
                    password: password.as_deref(),
                };
                topic::list_topics(&gateway_url, auth, json).await
            }
        },

        Commands::Produce {
            addr,
            topic,
            partition,
            class_id,
            payload,
            region,
            policy,
        } => {
            ctl::run_produce(
                &addr, &topic, partition, &class_id, &payload, &region, policy,
            )
            .await
        }

        Commands::Consume {
            addr,
            topic,
            partition,
            class_id,
            offset,
            max_records,
        }
        | Commands::Fetch {
            addr,
            topic,
            partition,
            class_id,
            offset,
            max_records,
        } => ctl::run_consume(&addr, &topic, partition, &class_id, offset, max_records).await,

        Commands::Policy { cmd } => match cmd {
            PolicyCommands::Sign {
                policy,
                key,
                output,
            } => policy_cmd::sign_policy(&policy, &key, output.as_deref()),

            PolicyCommands::Verify {
                policy,
                trusted_keys,
            } => policy_cmd::verify_policy(&policy, &trusted_keys),

            PolicyCommands::Publish {
                policy,
                gateway_url,
            } => policy_cmd::publish_policy(&policy, &gateway_url).await,
        },

        Commands::VerifyAudit {
            log_path,
            stream,
            pubkey,
            json,
        } => ctl::run_verify_audit(&log_path, stream.as_deref(), pubkey.as_deref(), json).await,

        Commands::Benchmark { count, concurrency } => ctl::run_benchmark(count, concurrency).await,

        Commands::Demo => ctl::run_demo().await,

        Commands::Promote {
            addr,
            topic,
            partition,
            class_id,
            min_watermark,
        } => ctl::run_promote(&addr, &topic, partition, &class_id, min_watermark).await,

        Commands::Watermark {
            addr,
            topic,
            partition,
            source_region,
        } => ctl::run_watermark(&addr, &topic, partition, &source_region).await,

        Commands::Completion { shell } => {
            let mut cmd = Cli::command();
            completion::generate_completion(shell, &mut cmd);
            0
        }
    };

    std::process::exit(exit_code);
}
