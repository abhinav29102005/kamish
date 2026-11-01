use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, Debug)]
pub struct AuthOptions<'a> {
    pub token: Option<&'a str>,
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub status: String,
    pub version: String,
    pub region: String,
    pub mode: String,
    pub known_peers: Vec<String>,
    pub degraded_links: Vec<String>,
    pub max_frame_bytes: usize,
    pub max_payload_bytes: usize,
    pub max_connections: usize,
    pub timestamp_utc: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorityStatus {
    pub authority_type: String,
    pub consensus_backing: String,
    pub quorum_backed: bool,
    pub active_region: String,
    pub monotonic_clock_ms: u64,
    pub active_leases_count: usize,
    pub total_quarantined_records: usize,
}

pub async fn run_status(gateway_url: &str, auth: AuthOptions<'_>, json_output: bool) -> i32 {
    let token = match get_or_login_token(gateway_url, auth).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Authentication error querying Kamish gateway at '{gateway_url}': {e}");
            eprintln!(
                "Ensure 'kamish start' is running or specify credentials with --username and --password."
            );
            return 1;
        }
    };

    match fetch_cluster_telemetry(gateway_url, &token).await {
        Ok((status, auth)) => {
            if json_output {
                let combined = serde_json::json!({
                    "cluster": status,
                    "authority": auth,
                });
                println!("{}", serde_json::to_string_pretty(&combined).unwrap());
            } else {
                render_status_table(&status, &auth);
            }
            0
        }
        Err(e) => {
            eprintln!("Error querying Kamish gateway at '{gateway_url}': {e}");
            1
        }
    }
}

pub async fn get_or_login_token(
    gateway_url: &str,
    auth: AuthOptions<'_>,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(t) = auth.token {
        return Ok(t.to_string());
    }

    let username = auth.username.unwrap_or("admin");
    let password = if let Some(p) = auth.password {
        p.to_string()
    } else if let Ok(p) = std::env::var("KAMISH_ADMIN_PASSWORD") {
        p
    } else {
        "kamish-admin-2026".to_string()
    };

    let base = gateway_url.trim_end_matches('/');
    let login_url = format!("{base}/api/auth/login");
    let payload = serde_json::json!({
        "username": username,
        "password": password,
    });

    let res = post_json_raw(&login_url, &payload).await?;
    let val: serde_json::Value = serde_json::from_slice(&res)?;
    if let Some(tok) = val
        .get("session_token")
        .or_else(|| val.get("token"))
        .and_then(|t| t.as_str())
    {
        Ok(tok.to_string())
    } else {
        Err(format!(
            "Login failed: {}",
            val.get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("invalid credentials")
        )
        .into())
    }
}

async fn fetch_cluster_telemetry(
    gateway_url: &str,
    token: &str,
) -> Result<(ClusterStatus, AuthorityStatus), Box<dyn std::error::Error>> {
    let base = gateway_url.trim_end_matches('/');
    let url_status = format!("{base}/api/status");
    let url_auth = format!("{base}/api/authority");

    let status_json: ClusterStatus = fetch_authenticated_json(&url_status, token).await?;
    let auth_json: AuthorityStatus = fetch_authenticated_json(&url_auth, token).await?;

    Ok((status_json, auth_json))
}

pub async fn fetch_authenticated_json<T: for<'de> Deserialize<'de>>(
    url_str: &str,
    token: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    let parsed: axum::http::Uri = url_str.parse()?;
    let host = parsed.host().unwrap_or("127.0.0.1");
    let port = parsed.port_u16().unwrap_or(3001);
    let path = parsed.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

    let stream = tokio::net::TcpStream::connect(format!("{host}:{port}")).await?;
    let io = hyper_util::rt::TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = hyper::Request::builder()
        .uri(path)
        .header(hyper::header::HOST, host)
        .header(hyper::header::USER_AGENT, "kamish-cli")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(hyper::header::COOKIE, format!("kamish_session={token}"))
        .body(http_body_util::Empty::<hyper::body::Bytes>::new())?;

    let res = sender.send_request(req).await?;
    let bytes = http_body_util::BodyExt::collect(res.into_body())
        .await?
        .to_bytes();
    let val: T = serde_json::from_slice(&bytes)?;
    Ok(val)
}

pub async fn post_json_raw(
    url_str: &str,
    body: &serde_json::Value,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let parsed: axum::http::Uri = url_str.parse()?;
    let host = parsed.host().unwrap_or("127.0.0.1");
    let port = parsed.port_u16().unwrap_or(3001);
    let path = parsed.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

    let stream = tokio::net::TcpStream::connect(format!("{host}:{port}")).await?;
    let io = hyper_util::rt::TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    tokio::spawn(async move {
        let _ = conn.await;
    });

    let bytes = serde_json::to_vec(body)?;
    let req = hyper::Request::builder()
        .method(hyper::Method::POST)
        .uri(path)
        .header(hyper::header::HOST, host)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(http_body_util::Full::new(hyper::body::Bytes::from(bytes)))?;

    let res = sender.send_request(req).await?;
    let resp_bytes = http_body_util::BodyExt::collect(res.into_body())
        .await?
        .to_bytes()
        .to_vec();
    Ok(resp_bytes)
}

fn render_status_table(status: &ClusterStatus, auth: &AuthorityStatus) {
    let use_color = std::env::var("NO_COLOR").is_err();
    let green = if use_color { "\x1b[1;32m" } else { "" };
    let cyan = if use_color { "\x1b[1;36m" } else { "" };
    let yellow = if use_color { "\x1b[1;33m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    println!("{cyan}=== Kamish Cluster Status ==={reset}");
    println!("Status          : {green}{}{reset}", status.status);
    println!("Version         : {}", status.version);
    println!("Active Region   : {cyan}{}{reset}", status.region);
    println!("Operating Mode  : {}", status.mode);
    println!("Known Peers     : {:?}", status.known_peers);
    println!("Degraded Links  : {:?}", status.degraded_links);
    println!("Max Frame Bytes : {}", status.max_frame_bytes);
    println!("--------------------------------------------------");
    println!("{cyan}=== Sovereignty & Consensus Authority ==={reset}");
    println!("Authority Type  : {yellow}{}{reset}", auth.authority_type);
    println!("Consensus Model : {}", auth.consensus_backing);
    println!(
        "Quorum Backed   : {}",
        if auth.quorum_backed {
            format!("{green}true{reset}")
        } else {
            format!("{yellow}false (single-node local storage){reset}")
        }
    );
    println!("Monotonic Clock : {} ms", auth.monotonic_clock_ms);
    println!("Active Leases   : {}", auth.active_leases_count);
    println!("Quarantined     : {}", auth.total_quarantined_records);
    println!("Timestamp (UTC) : {}\n", status.timestamp_utc);
}
