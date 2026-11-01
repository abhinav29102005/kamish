use ed25519_dalek::SigningKey;
use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyDocument, SignedPolicy};
use rand::Rng;
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct InitOptions<'a> {
    pub dir: &'a str,
    pub secret_file: Option<&'a str>,
    pub admin_user: &'a str,
    pub admin_password: Option<&'a str>,
    pub region: &'a str,
    pub force: bool,
}

pub fn run_init(opts: InitOptions) -> i32 {
    let out_dir = PathBuf::from(opts.dir);
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!(
            "Error: Cannot create directory '{}': {e}",
            out_dir.display()
        );
        return 1;
    }

    let config_file = out_dir.join("kamish.toml");
    let users_file = out_dir.join("users.txt");
    let key_file = out_dir.join("admin.key");
    let pubkey_file = out_dir.join("admin.pub");
    let policy_file = out_dir.join("policy.signed.json");

    if !opts.force {
        let existing = [
            &config_file,
            &users_file,
            &key_file,
            &pubkey_file,
            &policy_file,
        ]
        .into_iter()
        .filter(|p| p.exists())
        .collect::<Vec<_>>();

        if !existing.is_empty() {
            eprintln!(
                "Error: Configuration files already exist in '{}':",
                out_dir.display()
            );
            for p in existing {
                eprintln!("  - {}", p.display());
            }
            eprintln!("Refusing to overwrite existing files. Pass --force to overwrite.");
            return 1;
        }
    }

    // 1. Determine Admin Password
    let (admin_password, is_generated) = if let Some(secret_path) = opts.secret_file {
        match std::fs::read_to_string(secret_path) {
            Ok(content) => {
                let trimmed = content.trim().to_string();
                if trimmed.is_empty() {
                    eprintln!("Error: Secret file '{secret_path}' is empty.");
                    return 1;
                }
                // Support either direct password or user:password format
                if let Some((_, pw)) = trimmed.split_once(':') {
                    (pw.trim().to_string(), false)
                } else {
                    (trimmed, false)
                }
            }
            Err(e) => {
                eprintln!("Error reading secret file '{secret_path}': {e}");
                return 1;
            }
        }
    } else if let Some(pw) = opts.admin_password {
        (pw.to_string(), false)
    } else {
        // Generate secure 16-character alphanumeric password
        const CHARSET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%";
        let mut rng = rand::thread_rng();
        let gen_pw: String = (0..16)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        (gen_pw, true)
    };

    // 2. Generate Ed25519 Authority Signing Key
    let signing_key = SigningKey::generate(&mut OsRng);
    let key_bytes = signing_key.to_bytes();
    let pubkey_bytes = signing_key.verifying_key().to_bytes();
    let key_hex = hex::encode(key_bytes);
    let pubkey_hex = hex::encode(pubkey_bytes);

    // 3. Author default signed dev policy
    let regions = [opts.region, "US_EAST", "AP_SOUTH"];
    let mut legal = HashSet::new();
    let mut modes = HashMap::new();
    for r in &regions {
        legal.insert(r.to_string());
        modes.insert(r.to_string(), DestinationMode::Cleartext);
    }

    let mut classes = HashMap::new();
    for class_id in &["sovereign-class", "general", "fintech-transfer"] {
        classes.insert(
            class_id.to_string(),
            ClassRule {
                class_id: class_id.to_string(),
                description: format!("Default policy rule for {class_id}"),
                legal_regions: legal.clone(),
                destination_modes: modes.clone(),
                ack_mode: AckMode::LocalMajority,
                failover_candidates: regions.iter().map(|s| s.to_string()).collect(),
                transit_allowed: true,
            },
        );
    }

    let now_sec = chrono::Utc::now().timestamp();
    let doc = PolicyDocument {
        policy_id: "kamish-default-dev-policy".to_string(),
        version: 1,
        issued_at_sec: now_sec,
        ttl_seconds: 31_536_000, // 1 year
        issuer_pubkey_hex: pubkey_hex.clone(),
        classes,
        transit_rules: HashMap::new(),
    };

    let signed_policy = SignedPolicy::sign(doc, &signing_key);
    let policy_json = match serde_json::to_string_pretty(&signed_policy) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("Error serializing policy document: {e}");
            return 1;
        }
    };

    // 4. Write Files
    // users.txt
    let users_content = format!(
        "# Kamish Credentials File\n# format: username:password:role:tenant\n{}:{}:admin:tenant-sovereign\nauditor:auditor-pass-2026:auditor:tenant-sovereign\nproducer:producer-pass-2026:producer:tenant-default\nconsumer:consumer-pass-2026:consumer:tenant-default\n",
        opts.admin_user, admin_password
    );

    // kamish.toml
    let toml_content = format!(
        r#"# Kamish Sovereign Event Streaming Daemon Configuration

[cluster]
region = "{region}"
data_dir = "./data/kamish"
peers = ["US_EAST", "AP_SOUTH"]

[broker]
port = 9092
host = "127.0.0.1"
ack_level = "LeaderFsync"

[gateway]
port = 3001
host = "127.0.0.1"
users_file = "{users_path}"

[policy]
policy_file = "{policy_path}"
trusted_keys_file = "{pubkey_path}"
policy_floor = 1
"#,
        region = opts.region,
        users_path = users_file.display(),
        policy_path = policy_file.display(),
        pubkey_path = pubkey_file.display()
    );

    if let Err(e) = std::fs::write(&config_file, toml_content) {
        eprintln!("Error writing '{}': {e}", config_file.display());
        return 1;
    }

    if let Err(e) = std::fs::write(&users_file, users_content) {
        eprintln!("Error writing '{}': {e}", users_file.display());
        return 1;
    }

    if let Err(e) = std::fs::write(&key_file, format!("{key_hex}\n")) {
        eprintln!("Error writing '{}': {e}", key_file.display());
        return 1;
    }

    if let Err(e) = std::fs::write(&pubkey_file, format!("{pubkey_hex}\n")) {
        eprintln!("Error writing '{}': {e}", pubkey_file.display());
        return 1;
    }

    if let Err(e) = std::fs::write(&policy_file, policy_json) {
        eprintln!("Error writing '{}': {e}", policy_file.display());
        return 1;
    }

    let use_color = std::env::var("NO_COLOR").is_err();
    let green = if use_color { "\x1b[1;32m" } else { "" };
    let cyan = if use_color { "\x1b[1;36m" } else { "" };
    let yellow = if use_color { "\x1b[1;33m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    println!(
        "{green}[kamish init]{reset} Initialized Kamish sovereign cluster in {cyan}'{}'{reset}",
        out_dir.display()
    );
    println!("  Configuration : {}", config_file.display());
    println!("  Credentials   : {}", users_file.display());
    println!("  Policy Key    : {}", key_file.display());
    println!("  Public Key    : {}", pubkey_file.display());
    println!("  Signed Policy : {}", policy_file.display());
    println!("  Admin User    : {cyan}{}{reset}", opts.admin_user);

    if is_generated {
        println!(
            "  Admin Password: {yellow}{}{reset} (save this securely; no default credentials ship)",
            admin_password
        );
    } else {
        println!("  Admin Password: [configured from secret/flag]");
    }

    println!("\nNext steps:");
    println!("  1. Run 'kamish doctor' to verify system prerequisites.");
    println!("  2. Run 'kamish start' to start the broker, gateway, and dashboard.\n");

    0
}
