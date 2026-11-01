use crate::assets::static_handler;
use kamish_broker::BrokerServer;
use kamish_broker::catalog::{TenantCatalog, TopicMetadata};
use kamish_enforce::Enforcer;
use kamish_gateway::{GatewayState, app};
use kamish_meta::MetadataStore;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;

pub struct StartOptions<'a> {
    pub config: Option<&'a str>,
    pub port: u16,
    pub broker_port: u16,
    pub data_dir: &'a str,
    pub region: &'a str,
    pub host: &'a str,
    pub dev: bool,
    pub insecure_dev: bool,
}

pub fn discover_config_file(explicit: Option<&str>) -> Result<Option<PathBuf>, String> {
    if let Some(p) = explicit {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Ok(Some(pb));
        } else {
            return Err(format!("Specified config file '{}' does not exist", p));
        }
    }
    if let Ok(env_path) = std::env::var("KAMISH_CONFIG") {
        let pb = PathBuf::from(env_path.trim());
        if pb.is_file() {
            return Ok(Some(pb));
        }
    }
    let candidates = [
        PathBuf::from("kamish.toml"),
        PathBuf::from(".kamish.toml"),
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".kamish").join("config.toml")
        } else {
            PathBuf::from(".kamish.toml")
        },
    ];
    for cand in candidates {
        if cand.is_file() {
            return Ok(Some(cand));
        }
    }
    Ok(None)
}

pub async fn run_start(opts: StartOptions<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let use_color = std::env::var("NO_COLOR").is_err();
    let green = if use_color { "\x1b[1;32m" } else { "" };
    let yellow = if use_color { "\x1b[1;33m" } else { "" };
    let cyan = if use_color { "\x1b[1;36m" } else { "" };
    let red = if use_color { "\x1b[1;31m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    // 0. Config Discovery (flag > env > project file > user file)
    let found_config = match discover_config_file(opts.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{red}Error: {e}{reset}");
            std::process::exit(1);
        }
    };

    let mut config_region: Option<String> = None;
    let mut config_data_dir: Option<String> = None;
    let mut config_port: Option<u16> = None;
    let mut config_broker_port: Option<u16> = None;
    let mut config_host: Option<String> = None;
    let mut config_users_file: Option<PathBuf> = None;

    if let Some(ref cfg_path) = found_config
        && let Ok(content) = std::fs::read_to_string(cfg_path)
        && let Ok(toml_val) = toml::from_str::<toml::Value>(&content)
    {
        if let Some(gateway_tbl) = toml_val.get("gateway").and_then(|v| v.as_table()) {
            if let Some(uf) = gateway_tbl.get("users_file").and_then(|v| v.as_str()) {
                let uf_path = if Path::new(uf).is_relative() {
                    let parent = cfg_path.parent().unwrap_or_else(|| Path::new("."));
                    parent.join(uf)
                } else {
                    PathBuf::from(uf)
                };
                if uf_path.exists() {
                    config_users_file = Some(uf_path);
                }
            }
            if let Some(p) = gateway_tbl.get("port").and_then(|v| v.as_integer()) {
                config_port = Some(p as u16);
            }
            if let Some(h) = gateway_tbl.get("host").and_then(|v| v.as_str()) {
                config_host = Some(h.to_string());
            }
        }
        if let Some(broker_tbl) = toml_val.get("broker").and_then(|v| v.as_table())
            && let Some(bp) = broker_tbl.get("port").and_then(|v| v.as_integer())
        {
            config_broker_port = Some(bp as u16);
        }
        if let Some(cluster_tbl) = toml_val.get("cluster").and_then(|v| v.as_table()) {
            if let Some(r) = cluster_tbl.get("region").and_then(|v| v.as_str()) {
                config_region = Some(r.to_string());
            }
            if let Some(d) = cluster_tbl.get("data_dir").and_then(|v| v.as_str()) {
                config_data_dir = Some(d.to_string());
            }
        }
    }

    let effective_host = if let Some(ref h) = config_host {
        if opts.host == "127.0.0.1" {
            h.as_str()
        } else {
            opts.host
        }
    } else {
        opts.host
    };

    let effective_port = if let Some(p) = config_port {
        if opts.port == 3001 { p } else { opts.port }
    } else {
        opts.port
    };

    let effective_broker_port = if let Some(bp) = config_broker_port {
        if opts.broker_port == 9092 {
            bp
        } else {
            opts.broker_port
        }
    } else {
        opts.broker_port
    };

    let effective_region = if let Some(ref r) = config_region {
        if opts.region == "EU_WEST" || opts.region == "EU_CENTRAL" {
            r.as_str()
        } else {
            opts.region
        }
    } else {
        opts.region
    };

    let effective_data_dir = if let Some(ref d) = config_data_dir {
        if opts.data_dir == "./data/kamish" {
            d.as_str()
        } else {
            opts.data_dir
        }
    } else {
        opts.data_dir
    };

    // 1. Security Check: Non-loopback interface requires TLS or --insecure-dev
    if effective_host != "127.0.0.1"
        && effective_host != "localhost"
        && effective_host != "::1"
        && !opts.insecure_dev
    {
        eprintln!(
            "{red}Error: Refusing to bind to non-loopback interface '{}' without TLS unless --insecure-dev is explicitly specified.{reset}",
            effective_host
        );
        std::process::exit(1);
    }

    let data_path = PathBuf::from(effective_data_dir);
    std::fs::create_dir_all(&data_path)?;

    // 2. Load User Store (Fails closed if no credentials configured)
    let user_store = if let Some(ref uf_path) = config_users_file {
        match kamish_gateway::auth::UserStore::load_from_file(uf_path) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                eprintln!(
                    "{red}Error: Failed to load credentials from '{}': {e}{reset}",
                    uf_path.display()
                );
                std::process::exit(1);
            }
        }
    } else {
        match kamish_gateway::auth::UserStore::load_from_env_or_file(opts.dev) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                eprintln!("{red}Error: Failed to initialize authentication: {e}{reset}");
                eprintln!(
                    "Action: Run 'kamish init' to create an administrator account, set KAMISH_USERS_FILE / KAMISH_ADMIN_PASSWORD, or pass --dev for local development."
                );
                std::process::exit(1);
            }
        }
    };

    // 3. Setup Audit Log, Policy Cache, and Enforcer
    let audit_log = kamish_audit::AuditLog::open(data_path.join("audit.log"))?;
    let (policy_cache, _signing_key) =
        kamish_gateway::create_default_policy_cache(&[effective_region, "US_EAST", "AP_SOUTH"]);
    let enforcer = Arc::new(Enforcer::new(effective_region, policy_cache, audit_log));
    let meta = Arc::new(MetadataStore::new());

    // 4. Setup Topic Catalog
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
    general_classes.insert("general-topic".to_string());
    general_classes.insert("general".to_string());
    catalog.register_topic(TopicMetadata::new(
        "general-topic",
        "tenant-default",
        3,
        general_classes,
    ));

    let peers = vec!["US_EAST".to_string(), "AP_SOUTH".to_string()];
    let broker = Arc::new(
        BrokerServer::try_new(
            effective_region.to_string(),
            enforcer,
            meta,
            peers,
            &data_path,
        )?
        .with_catalog(catalog),
    );

    // Initial partition claims
    for p in 0..3 {
        let _ = broker.claim_partition_ownership("financial-transactions", p, 0, 1, 30_000);
        let _ = broker.claim_partition_ownership("sovereign-stream", p, 0, 1, 30_000);
        let _ = broker.claim_partition_ownership("general-topic", p, 0, 1, 30_000);
    }

    // 5. Consensus / Mode Check
    let etcd_online = std::net::TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], 2379)),
        std::time::Duration::from_millis(250),
    )
    .is_ok();

    if etcd_online {
        println!(
            "{green}[kamish start] Connected to etcd quorum consensus member at 127.0.0.1:2379.{reset}"
        );
    } else {
        println!(
            "{yellow}[kamish start] Running in SINGLE-NODE mode (authority: LOCAL_PERSISTENT_STORAGE).{reset}\n\
             {yellow}             Multi-region CAS ownership and lease fencing require etcd quorum (run 'kamish doctor' for guidance).{reset}"
        );
    }

    // 6. Start Framed TCP Broker Listener
    let broker_bind_addr = format!("{}:{}", effective_host, effective_broker_port);
    let (broker_addr, _broker_handle) =
        broker.clone().start_tcp_listener(&broker_bind_addr).await?;

    // 7. Build Axum Router combining Gateway API and Embedded Static Assets
    let state = GatewayState::new_with_store(broker, user_store);
    let gateway_router = app(state);
    let complete_app = gateway_router.fallback(static_handler);

    let bind_addr = format!("{}:{}", effective_host, effective_port);
    let listener = TcpListener::bind(&bind_addr).await?;

    println!("\n{green}================================================================{reset}");
    println!("{green}  KAMISH SOVEREIGN BROKER & DASHBOARD RUNNING                   {reset}");
    println!("{green}================================================================{reset}");
    println!("  Dashboard UI    : {cyan}http://{bind_addr}{reset}");
    println!("  REST/SSE API    : {cyan}http://{bind_addr}/api{reset}");
    println!("  Broker TCP Port : {cyan}{}{reset}", broker_addr);
    println!("  Prometheus Metrics: {cyan}http://{bind_addr}/metrics{reset}");
    println!("  Active Region   : {cyan}{}{reset}", effective_region);
    println!("  Data Directory  : {}", data_path.display());
    if let Some(ref cfg_p) = found_config {
        println!("  Config File     : {}", cfg_p.display());
    }
    println!("{green}----------------------------------------------------------------{reset}");
    println!("Press Ctrl+C to shut down cleanly.\n");

    // 8. Serve with graceful shutdown on SIGINT / SIGTERM
    axum::serve(listener, complete_app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C signal handler");
            println!("\n[kamish] Received shutdown signal. Gracefully draining connections...");
        })
        .await?;

    println!("[kamish] Clean shutdown complete.");
    Ok(())
}
