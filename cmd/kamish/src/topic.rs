use crate::status::{AuthOptions, fetch_authenticated_json, get_or_login_token};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TopicInfo {
    pub name: String,
    pub tenant_id: String,
    pub partition_count: usize,
    pub authorized_classes: Vec<String>,
}

pub async fn list_topics(gateway_url: &str, auth: AuthOptions<'_>, json_output: bool) -> i32 {
    let token = match get_or_login_token(gateway_url, auth).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Authentication error querying topics at '{gateway_url}': {e}");
            return 1;
        }
    };

    let url = format!("{}/api/topics", gateway_url.trim_end_matches('/'));
    match fetch_authenticated_topics(&url, &token).await {
        Ok(topics) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&topics).unwrap());
            } else {
                let use_color = std::env::var("NO_COLOR").is_err();
                let cyan = if use_color { "\x1b[1;36m" } else { "" };
                let reset = if use_color { "\x1b[0m" } else { "" };

                println!("{cyan}=== Registered Sovereign Topics ==={reset}");
                println!(
                    "{:<24} {:<18} {:<12} {:<20}",
                    "TOPIC", "TENANT", "PARTITIONS", "CLASSES"
                );
                println!("----------------------------------------------------------------------");
                for t in &topics {
                    println!(
                        "{:<24} {:<18} {:<12} {:?}",
                        t.name, t.tenant_id, t.partition_count, t.authorized_classes
                    );
                }
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("Error querying topics from '{gateway_url}': {e}");
            1
        }
    }
}

pub async fn create_topic(
    gateway_url: &str,
    name: &str,
    tenant_id: &str,
    partitions: usize,
    classes: &[String],
    auth: AuthOptions<'_>,
) -> i32 {
    let token = match get_or_login_token(gateway_url, auth).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Authentication error creating topic at '{gateway_url}': {e}");
            return 1;
        }
    };

    let url = format!("{}/api/topics", gateway_url.trim_end_matches('/'));
    let payload = serde_json::json!({
        "name": name,
        "tenant_id": tenant_id,
        "partition_count": partitions,
        "authorized_classes": classes,
    });

    match post_authenticated_json(&url, &payload, &token).await {
        Ok(_) => {
            println!(
                "[kamish topic create] Topic '{name}' successfully registered for tenant '{tenant_id}' ({partitions} partitions)."
            );
            0
        }
        Err(e) => {
            eprintln!("Error creating topic '{name}': {e}");
            1
        }
    }
}

async fn fetch_authenticated_topics(
    url_str: &str,
    token: &str,
) -> Result<Vec<TopicInfo>, Box<dyn std::error::Error>> {
    let val: serde_json::Value = fetch_authenticated_json(url_str, token).await?;
    let mut list = Vec::new();
    if let Some(arr) = val.get("topics").and_then(|v| v.as_array()) {
        for item in arr {
            let name = item["topic"]
                .as_str()
                .or_else(|| item["name"].as_str())
                .unwrap_or("")
                .to_string();
            let tenant = item["tenant_id"].as_str().unwrap_or("default").to_string();
            let p_count = item["partition_count"].as_u64().unwrap_or(1) as usize;
            let classes: Vec<String> = item["authorized_classes"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            list.push(TopicInfo {
                name,
                tenant_id: tenant,
                partition_count: p_count,
                authorized_classes: classes,
            });
        }
    }
    Ok(list)
}

async fn post_authenticated_json(
    url_str: &str,
    body: &serde_json::Value,
    token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let parsed: axum::http::Uri = url_str.parse()?;
    let host = parsed.host().unwrap_or("127.0.0.1");
    let port = parsed.port_u16().unwrap_or(3001);
    let path = parsed
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/api/topics");

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
        .header(hyper::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(hyper::header::COOKIE, format!("kamish_session={token}"))
        .header("x-csrf-token", token)
        .body(http_body_util::Full::new(hyper::body::Bytes::from(bytes)))?;

    let res = sender.send_request(req).await?;
    if !res.status().is_success() {
        return Err(format!("Gateway returned status {}", res.status()).into());
    }
    Ok(())
}
