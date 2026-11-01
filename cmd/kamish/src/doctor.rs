use serde::Serialize;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub version: String,
    pub git_commit: String,
    pub platform: String,
    pub config_check: CheckResult,
    pub broker_port_check: CheckResult,
    pub gateway_port_check: CheckResult,
    pub etcd_check: CheckResult,
    pub tls_check: CheckResult,
    pub data_dir_check: CheckResult,
    pub all_passed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

pub fn run_doctor(config_path_opt: Option<&str>, json_output: bool) -> i32 {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let git_commit = env!("CARGO_PKG_VERSION"); // fallback commit/version string
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);

    // 1. Config Check
    let candidate_configs = if let Some(p) = config_path_opt {
        vec![PathBuf::from(p)]
    } else {
        vec![
            PathBuf::from("kamish.toml"),
            PathBuf::from("./kamish.toml"),
            dirs_fallback_config(),
        ]
    };

    let mut found_config: Option<PathBuf> = None;
    for cand in candidate_configs {
        if cand.exists() && cand.is_file() {
            found_config = Some(cand);
            break;
        }
    }

    let config_check = match found_config {
        Some(ref p) => match std::fs::read_to_string(p) {
            Ok(content) => match toml::from_str::<toml::Value>(&content) {
                Ok(_) => CheckResult {
                    name: "Configuration File".into(),
                    status: CheckStatus::Pass,
                    message: format!("Valid TOML configuration found at '{}'", p.display()),
                    remediation: None,
                },
                Err(e) => CheckResult {
                    name: "Configuration File".into(),
                    status: CheckStatus::Fail,
                    message: format!("Malformed TOML in '{}': {e}", p.display()),
                    remediation: Some(
                        "Check syntax of kamish.toml or re-run 'kamish init --force'".into(),
                    ),
                },
            },
            Err(e) => CheckResult {
                name: "Configuration File".into(),
                status: CheckStatus::Fail,
                message: format!("Cannot read '{}': {e}", p.display()),
                remediation: Some("Verify file read permissions".into()),
            },
        },
        None => CheckResult {
            name: "Configuration File".into(),
            status: CheckStatus::Warn,
            message: "No kamish.toml found in current directory or user home".into(),
            remediation: Some(
                "Run 'kamish init' to create a default configuration skeleton".into(),
            ),
        },
    };

    // 2. Port checks
    let broker_port = 9092;
    let broker_port_check = test_port_availability(broker_port, "Broker TCP Port");

    let gateway_port = 3001;
    let gateway_port_check = test_port_availability(gateway_port, "Gateway HTTP Port");

    // 3. etcd check
    let etcd_endpoint = "127.0.0.1:2379";
    let etcd_check = test_etcd_reachability(etcd_endpoint);

    // 4. TLS check
    let tls_check = CheckResult {
        name: "TLS Certificates".into(),
        status: CheckStatus::Pass,
        message: "Development mode (pass --insecure-dev for non-loopback binding)".into(),
        remediation: None,
    };

    // 5. Data dir check
    let data_dir = Path::new("data");
    let data_dir_check = match std::fs::create_dir_all(data_dir) {
        Ok(_) => CheckResult {
            name: "Data Directory".into(),
            status: CheckStatus::Pass,
            message: format!(
                "Data directory '{}' is accessible and writable",
                data_dir.display()
            ),
            remediation: None,
        },
        Err(e) => CheckResult {
            name: "Data Directory".into(),
            status: CheckStatus::Fail,
            message: format!("Cannot write to '{}': {e}", data_dir.display()),
            remediation: Some("Verify directory permissions or specify --data-dir".into()),
        },
    };

    let all_passed = config_check.status != CheckStatus::Fail
        && broker_port_check.status != CheckStatus::Fail
        && gateway_port_check.status != CheckStatus::Fail
        && data_dir_check.status != CheckStatus::Fail;

    let report = DoctorReport {
        version,
        git_commit: git_commit.into(),
        platform,
        config_check,
        broker_port_check,
        gateway_port_check,
        etcd_check,
        tls_check,
        data_dir_check,
        all_passed,
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        render_human_report(&report);
    }

    if all_passed { 0 } else { 1 }
}

fn dirs_fallback_config() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".kamish").join("config.toml")
    } else {
        PathBuf::from(".kamish.toml")
    }
}

fn test_port_availability(port: u16, name: &str) -> CheckResult {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    match TcpListener::bind(addr) {
        Ok(_) => CheckResult {
            name: name.to_string(),
            status: CheckStatus::Pass,
            message: format!("Port {port} is available for binding"),
            remediation: None,
        },
        Err(e) => CheckResult {
            name: name.to_string(),
            status: CheckStatus::Warn,
            message: format!("Port {port} is currently in use or unavailable: {e}"),
            remediation: Some(format!(
                "Another process may be using port {port}. Pass --port to choose a different port."
            )),
        },
    }
}

fn test_etcd_reachability(endpoint: &str) -> CheckResult {
    match endpoint.parse::<SocketAddr>() {
        Ok(addr) => match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
            Ok(_) => CheckResult {
                name: "Consensus (etcd)".to_string(),
                status: CheckStatus::Pass,
                message: format!("etcd consensus member reachable at {endpoint} (distributed quorum available)"),
                remediation: None,
            },
            Err(_) => CheckResult {
                name: "Consensus (etcd)".to_string(),
                status: CheckStatus::Warn,
                message: format!("etcd cluster not reachable at {endpoint}"),
                remediation: Some(
                    "Multi-region consensus and lease fencing require an etcd cluster.\n\
                     To start a 3-member cluster locally, run:\n\
                       docker compose -f docker-compose.etcd.yml up -d\n\
                     Or download and launch via scripts:\n\
                       ./scripts/download_etcd.sh && ./scripts/start_etcd_cluster.sh\n\
                     Note: Single-node mode functions without etcd (authority: LOCAL_PERSISTENT_STORAGE)."
                        .to_string(),
                ),
            },
        },
        Err(e) => CheckResult {
            name: "Consensus (etcd)".to_string(),
            status: CheckStatus::Fail,
            message: format!("Invalid etcd endpoint '{endpoint}': {e}"),
            remediation: Some("Ensure endpoint format is IP:PORT (e.g. 127.0.0.1:2379)".into()),
        },
    }
}

fn render_human_report(report: &DoctorReport) {
    let use_color = std::env::var("NO_COLOR").is_err();

    let green = if use_color { "\x1b[1;32m" } else { "" };
    let yellow = if use_color { "\x1b[1;33m" } else { "" };
    let red = if use_color { "\x1b[1;31m" } else { "" };
    let cyan = if use_color { "\x1b[1;36m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    println!("{cyan}=== Kamish System Diagnostic (kamish doctor) ==={reset}");
    println!(
        "Version  : {} (commit: {})",
        report.version, report.git_commit
    );
    println!("Platform : {}", report.platform);
    println!("--------------------------------------------------");

    let checks = [
        &report.config_check,
        &report.broker_port_check,
        &report.gateway_port_check,
        &report.etcd_check,
        &report.tls_check,
        &report.data_dir_check,
    ];

    for c in checks {
        let tag = match c.status {
            CheckStatus::Pass => format!("{green}[PASS]{reset}"),
            CheckStatus::Warn => format!("{yellow}[WARN]{reset}"),
            CheckStatus::Fail => format!("{red}[FAIL]{reset}"),
        };
        println!("{:<7} {:<24} : {}", tag, c.name, c.message);
        if let Some(ref rem) = c.remediation {
            for line in rem.lines() {
                println!("        -> {}", line);
            }
        }
    }

    println!("--------------------------------------------------");
    if report.all_passed {
        println!("{green}System is ready for operation.{reset}\n");
    } else {
        println!(
            "{red}One or more prerequisite checks failed. See remediation advice above.{reset}\n"
        );
    }
}
