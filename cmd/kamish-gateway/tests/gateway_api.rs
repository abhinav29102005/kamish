use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use kamish_broker::BrokerServer;
use kamish_broker::catalog::{TenantCatalog, TopicMetadata};
use kamish_enforce::Enforcer;
use kamish_gateway::{GatewayState, app};
use kamish_meta::MetadataStore;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;

fn setup_test_gateway() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let audit_log = kamish_audit::AuditLog::open(dir.path().join("audit.log")).unwrap();
    let (policy_cache, _signing_key) =
        kamish_gateway::create_default_policy_cache(&["EU_WEST", "US_EAST", "AP_SOUTH"]);
    let enforcer = Arc::new(Enforcer::new("EU_WEST", policy_cache, audit_log));
    let meta = Arc::new(MetadataStore::new());

    let mut catalog = TenantCatalog::new();
    let mut sovereign_classes = HashSet::new();
    sovereign_classes.insert("sovereign-class".to_string());
    catalog.register_topic(TopicMetadata::new(
        "financial-transactions",
        "tenant-sovereign",
        3,
        sovereign_classes,
    ));

    let peers = vec!["US_EAST".to_string(), "AP_SOUTH".to_string()];
    let broker = Arc::new(
        BrokerServer::try_new("EU_WEST".to_string(), enforcer, meta, peers, dir.path())
            .unwrap()
            .with_catalog(catalog),
    );

    for p in 0..3 {
        let _ = broker.claim_partition_ownership("financial-transactions", p, 0, 1, 30_000);
    }

    let state = GatewayState::new(broker);
    (app(state), dir)
}

async fn response_json(response: axum::response::Response) -> Value {
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body_bytes).unwrap()
}

async fn login_user(app: &axum::Router, username: &str, password: &str) -> (String, String) {
    let login_req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"username":"{}","password":"{}"}}"#,
            username, password
        )))
        .unwrap();
    let res = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    (
        json["session_token"].as_str().unwrap().to_string(),
        json["csrf_token"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_auth_login_and_logout_lifecycle() {
    let (app, _dir) = setup_test_gateway();

    // 1. Unauthenticated request to /api/status -> 401
    let req = Request::builder()
        .uri("/api/status")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. Invalid login -> 401
    let login_req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"username":"admin","password":"wrong-password"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 3. Valid login as admin -> 200, sets cookie, returns csrf_token
    let login_req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"username":"admin","password":"kamish-admin-2026"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let cookie_header = res
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie_header.contains("kamish_session="));
    assert!(cookie_header.contains("HttpOnly"));

    let json = response_json(res).await;
    let csrf_token = json["csrf_token"].as_str().unwrap().to_string();
    let session_token = json["session_token"].as_str().unwrap().to_string();
    assert_eq!(json["user"]["role"], "admin");

    // 4. Authenticated request using cookie to /api/auth/me
    let me_req = Request::builder()
        .uri("/api/auth/me")
        .header(header::COOKIE, format!("kamish_session={session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(me_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let me_json = response_json(res).await;
    assert_eq!(me_json["user"]["username"], "admin");
    assert_eq!(me_json["user"]["role"], "admin");

    // 5. Authenticated request using Bearer header
    let me_bearer_req = Request::builder()
        .uri("/api/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(me_bearer_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 6. Logout -> invalidates session
    let logout_req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/logout")
        .header(header::COOKIE, format!("kamish_session={session_token}"))
        .header("x-csrf-token", &csrf_token)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(logout_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 7. Subsequent request -> 401
    let me_req2 = Request::builder()
        .uri("/api/auth/me")
        .header(header::COOKIE, format!("kamish_session={session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(me_req2).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_csrf_protection_and_bypass_prevention() {
    let (app, _dir) = setup_test_gateway();
    let (session, csrf) = login_user(&app, "admin", "kamish-admin-2026").await;

    // 1. POST without CSRF header -> 403 Forbidden
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/scenarios/run")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"scenario":"residency"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 2. POST with wrong CSRF header -> 403 Forbidden
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/scenarios/run")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .header("x-csrf-token", "bogus-csrf-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"scenario":"residency"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 3. POST with correct CSRF header -> 200 OK
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/scenarios/run")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"scenario":"residency"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_role_based_access_control() {
    let (app, _dir) = setup_test_gateway();

    // A. Consumer role
    let (consumer_session, consumer_csrf) =
        login_user(&app, "consumer", "kamish-consumer-2026").await;

    // Consumer CANNOT produce -> 403
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/produce")
        .header(header::COOKIE, format!("kamish_session={consumer_session}"))
        .header("x-csrf-token", &consumer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"topic":"financial-transactions","partition":0,"class_id":"general","payload":"test"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Consumer CANNOT publish policy -> 403
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/policy/publish")
        .header(header::COOKIE, format!("kamish_session={consumer_session}"))
        .header("x-csrf-token", &consumer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"policy_version":2}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Consumer CAN fetch -> 200
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/fetch")
        .header(header::COOKIE, format!("kamish_session={consumer_session}"))
        .header("x-csrf-token", &consumer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"topic":"financial-transactions","partition":0,"offset":0,"max_records":10}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // B. Producer role
    let (producer_session, producer_csrf) =
        login_user(&app, "producer", "kamish-producer-2026").await;

    // Producer CAN produce -> 200
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/produce")
        .header(header::COOKIE, format!("kamish_session={producer_session}"))
        .header("x-csrf-token", &producer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"topic":"financial-transactions","partition":0,"class_id":"sovereign-class","payload":"test-data"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Producer CANNOT publish policy -> 403
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/policy/publish")
        .header(header::COOKIE, format!("kamish_session={producer_session}"))
        .header("x-csrf-token", &producer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"policy_version":2}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // C. Auditor role
    let (auditor_session, auditor_csrf) = login_user(&app, "auditor", "kamish-auditor-2026").await;

    // Auditor CAN inspect audit log -> 200
    let req = Request::builder()
        .uri("/api/audit")
        .header(header::COOKIE, format!("kamish_session={auditor_session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Auditor CAN trigger audit verify -> 200
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/audit/verify")
        .header(header::COOKIE, format!("kamish_session={auditor_session}"))
        .header("x-csrf-token", &auditor_csrf)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Auditor CANNOT produce -> 403
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/produce")
        .header(header::COOKIE, format!("kamish_session={auditor_session}"))
        .header("x-csrf-token", &auditor_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"topic":"financial-transactions","partition":0,"class_id":"sovereign-class","payload":"data"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_real_telemetry_endpoints() {
    let (app, _dir) = setup_test_gateway();
    let (session, _) = login_user(&app, "admin", "kamish-admin-2026").await;

    // 1. /api/status
    let req = Request::builder()
        .uri("/api/status")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert_eq!(json["status"], "HEALTHY");
    assert_eq!(json["region"], "EU_WEST");
    assert_eq!(json["mode"], "LIVE");

    // 2. /api/streams
    let req = Request::builder()
        .uri("/api/streams")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert!(json["count"].as_u64().unwrap() >= 1);

    // 3. /api/authority
    let req = Request::builder()
        .uri("/api/authority")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert_eq!(json["active_region"], "EU_WEST");

    // 4. /api/policy
    let req = Request::builder()
        .uri("/api/policy")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert_eq!(json["policy_version_floor"], 1);

    // 5. /api/checkers
    let req = Request::builder()
        .uri("/api/checkers")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert_eq!(json["history_checker"]["status"], "PASS");
    assert_eq!(json["canary_scanner"]["status"], "PASS");

    // 6. /api/routing
    let req = Request::builder()
        .uri("/api/routing")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert!(json["active_routes"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn test_real_produce_and_fetch_end_to_end() {
    let (app, _dir) = setup_test_gateway();
    let (producer_session, producer_csrf) =
        login_user(&app, "producer", "kamish-producer-2026").await;
    let (consumer_session, consumer_csrf) =
        login_user(&app, "consumer", "kamish-consumer-2026").await;

    // Produce real record
    let produce_payload = "sovereign_gateway_test_event_9988";
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/produce")
        .header(header::COOKIE, format!("kamish_session={producer_session}"))
        .header("x-csrf-token", &producer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"topic":"financial-transactions","partition":0,"class_id":"sovereign-class","payload":"{}"}}"#,
            produce_payload
        )))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert_eq!(json["status"], "ACKNOWLEDGED");
    let produced_offset = json["base_offset"].as_u64().unwrap();

    // Fetch records at produced offset
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/fetch")
        .header(header::COOKIE, format!("kamish_session={consumer_session}"))
        .header("x-csrf-token", &consumer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"topic":"financial-transactions","partition":0,"offset":{},"max_records":5}}"#,
            produced_offset
        )))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let fetch_json = response_json(res).await;
    let records = fetch_json["records"].as_array().unwrap();
    assert!(!records.is_empty());
    assert_eq!(records[0]["payload"], produce_payload);
}

#[tokio::test]
async fn test_scenario_execution_and_results() {
    let (app, _dir) = setup_test_gateway();
    let (session, csrf) = login_user(&app, "admin", "kamish-admin-2026").await;

    for scenario in &["residency", "slowlink", "outage", "quorum"] {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/scenarios/run")
            .header(header::COOKIE, format!("kamish_session={session}"))
            .header("x-csrf-token", &csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(r#"{{"scenario":"{}"}}"#, scenario)))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let json = response_json(res).await;
        assert_eq!(json["status"], "PASS");
        assert_eq!(json["scenario"], *scenario);
    }
}

#[tokio::test]
async fn test_mode_toggle_live_vs_demo() {
    let (app, _dir) = setup_test_gateway();
    let (session, csrf) = login_user(&app, "admin", "kamish-admin-2026").await;

    // Initially LIVE
    let req = Request::builder()
        .uri("/api/mode")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(response_json(res).await["mode"], "LIVE");

    // Toggle to DEMO
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/mode")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"mode":"DEMO"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify mode is DEMO in /api/mode and /api/status
    let req = Request::builder()
        .uri("/api/mode")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response_json(res).await["mode"], "DEMO");

    let req = Request::builder()
        .uri("/api/status")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response_json(res).await["mode"], "DEMO");

    // Toggle back to LIVE
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/mode")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"mode":"LIVE"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(response_json(res).await["mode"], "LIVE");
}

#[tokio::test]
async fn test_audit_verify_hash_chain() {
    let (app, _dir) = setup_test_gateway();
    let (session, csrf) = login_user(&app, "auditor", "kamish-auditor-2026").await;

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/audit/verify")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .header("x-csrf-token", &csrf)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert_eq!(json["verification_result"], "PASS");
    assert_eq!(json["anchored_boundary"], true);
    assert_eq!(json["zero_tamper_detected"], true);
}

#[tokio::test]
async fn test_sliding_window_rate_limiting() {
    let (app, _dir) = setup_test_gateway();
    let (session, _) = login_user(&app, "admin", "kamish-admin-2026").await;

    let mut throttled = false;
    // Gateway rate limiter is configured for 120 req/min
    for _ in 0..125 {
        let req = Request::builder()
            .uri("/api/status")
            .header(header::COOKIE, format!("kamish_session={session}"))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        if res.status() == StatusCode::TOO_MANY_REQUESTS {
            throttled = true;
            break;
        }
    }
    assert!(
        throttled,
        "Rate limiter should throttle after burst exceeding limit"
    );
}
