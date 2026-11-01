use ed25519_dalek::SigningKey;
use kamish_policy::{
    PolicyDocument, SignedPolicy, load_trusted_keys_from_file, load_verified_policy_from_file,
};
use std::path::Path;

pub fn sign_policy(policy_path: &str, key_path: &str, output_path: Option<&str>) -> i32 {
    let key_hex = match std::fs::read_to_string(key_path) {
        Ok(k) => k.trim().to_string(),
        Err(e) => {
            eprintln!("Error reading private key '{key_path}': {e}");
            return 1;
        }
    };

    let key_bytes = match hex::decode(&key_hex) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            eprintln!("Error: Key in '{key_path}' is not a valid 32-byte hex-encoded seed.");
            return 1;
        }
    };

    let signing_key = SigningKey::from_bytes(&key_bytes);

    let doc_str = match std::fs::read_to_string(policy_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading policy document '{policy_path}': {e}");
            return 1;
        }
    };

    let doc: PolicyDocument = match serde_json::from_str(&doc_str) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error parsing policy document '{policy_path}': {e}");
            return 1;
        }
    };

    let signed = SignedPolicy::sign(doc, &signing_key);
    let output_json = match serde_json::to_string_pretty(&signed) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("Error serializing signed policy: {e}");
            return 1;
        }
    };

    if let Some(out) = output_path {
        if let Err(e) = std::fs::write(out, &output_json) {
            eprintln!("Error writing signed policy to '{out}': {e}");
            return 1;
        }
        println!("[kamish policy sign] Signed policy written to '{out}'");
    } else {
        println!("{output_json}");
    }

    0
}

pub fn verify_policy(policy_path: &str, trusted_keys_path: &str) -> i32 {
    let trusted_keys = match load_trusted_keys_from_file(Path::new(trusted_keys_path)) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Error loading trusted keys from '{trusted_keys_path}': {e}");
            return 1;
        }
    };

    let now_sec = chrono::Utc::now().timestamp();
    match load_verified_policy_from_file(policy_path, &trusted_keys, 1, now_sec) {
        Ok(signed_policy) => {
            let use_color = std::env::var("NO_COLOR").is_err();
            let green = if use_color { "\x1b[1;32m" } else { "" };
            let reset = if use_color { "\x1b[0m" } else { "" };
            println!(
                "{green}[PASS]{reset} Policy '{}' (version {}) cryptographically verified by authority key '{}'",
                signed_policy.policy.policy_id,
                signed_policy.policy.version,
                signed_policy.policy.issuer_pubkey_hex
            );
            0
        }
        Err(e) => {
            let use_color = std::env::var("NO_COLOR").is_err();
            let red = if use_color { "\x1b[1;31m" } else { "" };
            let reset = if use_color { "\x1b[0m" } else { "" };
            eprintln!("{red}[FAIL]{reset} Cryptographic policy verification failed: {e}");
            1
        }
    }
}

pub async fn publish_policy(policy_path: &str, gateway_url: &str) -> i32 {
    let policy_str = match std::fs::read_to_string(policy_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading policy file '{policy_path}': {e}");
            return 1;
        }
    };

    // Verify it parses as SignedPolicy before publishing
    if let Err(e) = serde_json::from_str::<SignedPolicy>(&policy_str) {
        eprintln!("Error: '{policy_path}' is not a valid signed policy: {e}");
        return 1;
    }

    let parsed_url = match axum::http::Uri::try_from(format!(
        "{}/api/policy/publish",
        gateway_url.trim_end_matches('/')
    )) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Invalid gateway URL '{gateway_url}': {e}");
            return 1;
        }
    };

    let host = parsed_url.host().unwrap_or("127.0.0.1");
    let port = parsed_url.port_u16().unwrap_or(3001);
    let path = parsed_url
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/api/policy/publish");

    let stream = match tokio::net::TcpStream::connect(format!("{host}:{port}")).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Cannot connect to gateway at {host}:{port}: {e}");
            return 1;
        }
    };

    let io = hyper_util::rt::TokioIo::new(stream);
    let (mut sender, conn) = match hyper::client::conn::http1::handshake(io).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("HTTP handshake failed: {e}");
            return 1;
        }
    };

    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = match hyper::Request::builder()
        .method(hyper::Method::POST)
        .uri(path)
        .header(hyper::header::HOST, host)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(http_body_util::Full::new(hyper::body::Bytes::from(
            policy_str,
        ))) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to construct publish request: {e}");
            return 1;
        }
    };

    match sender.send_request(req).await {
        Ok(res) if res.status().is_success() => {
            println!(
                "[kamish policy publish] Policy successfully published and activated on gateway cluster."
            );
            0
        }
        Ok(res) => {
            let status = res.status();
            eprintln!("Gateway rejected policy publication with status: {status}");
            1
        }
        Err(e) => {
            eprintln!("Failed to send policy to gateway: {e}");
            1
        }
    }
}
