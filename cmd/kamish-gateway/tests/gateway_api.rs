use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use kamish_broker::BrokerServer;
use kamish_broker::catalog::{TenantCatalog, TopicMetadata};
use kamish_enforce::Enforcer;
use kamish_gateway::{GatewayState, app};
use kamish_meta::{MetadataAuthority, MetadataStore};
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

    // Auditor CAN trigger audit verify -> 200 OK
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
    assert_eq!(
        json["authority_type"],
        "LOCAL_PERSISTENT_STORAGE"
    );
    assert_eq!(json["consensus_backing"], "LOCAL_FSYNC");
    assert_eq!(json["quorum_backed"], false);

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

    // 5. /api/checkers -> 200 OK (live invariant checkers and canary scanner)
    let req = Request::builder()
        .uri("/api/checkers")
        .header(header::COOKIE, format!("kamish_session={session}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    assert_eq!(json["status"], "PASS");
    assert_eq!(json["scope"]["target"], "live_partition_segments");
    assert_eq!(json["scope"]["max_records_per_partition"], 100);
    assert_eq!(json["limits"]["max_bytes_per_partition"], 10485760);
    assert_eq!(json["limits"]["max_records_total"], 10000);
    assert_eq!(json["canary_scanner"]["positive_control_verified"], true);

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

    let scenarios = [
        ("residency", "SIMULATED", true),
        ("slowlink", "PASS", false),
        ("outage", "SIMULATED", true),
        ("quorum", "NOT_IMPLEMENTED", true),
    ];
    for (scenario, expected_status, expected_simulated) in scenarios {
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
        assert_eq!(json["status"], expected_status);
        assert_eq!(json["scenario"], scenario);
        assert_eq!(json["simulated"], expected_simulated);
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
    assert_eq!(json["hash_chain_valid"], true);
    assert_eq!(json["zero_tamper_detected"], true);
}

#[tokio::test]
async fn test_audit_verification_tamper_and_unanchored_tail() {
    let dir = tempfile::tempdir().unwrap();
    let audit_file = dir.path().join("audit.log");

    // 1. Create audit log with 3 valid records
    {
        let log = kamish_audit::AuditLog::open(&audit_file).unwrap();
        log.record(
            kamish_audit::AuditAction::Ingress,
            "sovereign-class",
            "EU_WEST",
            "EU_WEST",
            1,
            kamish_audit::DecisionOutcome::AllowCleartext,
            "admission-grant-0",
        )
        .unwrap();
        log.record(
            kamish_audit::AuditAction::Ingress,
            "sovereign-class",
            "EU_WEST",
            "EU_WEST",
            1,
            kamish_audit::DecisionOutcome::AllowCleartext,
            "admission-grant-1",
        )
        .unwrap();
        log.record(
            kamish_audit::AuditAction::Ingress,
            "sovereign-class",
            "EU_WEST",
            "EU_WEST",
            1,
            kamish_audit::DecisionOutcome::AllowCleartext,
            "admission-grant-2",
        )
        .unwrap();
    }

    let meta = Arc::new(MetadataStore::new());
    let (policy_cache, _) = kamish_gateway::create_default_policy_cache(&["EU_WEST", "US_EAST"]);
    let audit_log = kamish_audit::AuditLog::open(&audit_file).unwrap();
    let _tail_hash_at_2 = audit_log.last_hash(); // at 3 records, but checkpoint will anchor at 2

    // Commit checkpoint at record_count = 2
    let stream = kamish_meta::StreamId::new("financial-transactions", 0);
    // Read the entries to get record 1 tail hash
    let entries = kamish_audit::AuditLog::read_verified_entries(&audit_file).unwrap();
    assert_eq!(entries.len(), 3);
    let anchored_tail = entries[1].entry_hash.clone();

    meta.commit_audit_checkpoint(kamish_meta::AuditCheckpoint {
        stream: stream.clone(),
        sequence: 1,
        tail_hash: anchored_tail,
        record_count: 2,
        signature_hex: "dummy-sig".to_string(),
        witness_timestamp_ns: 1234567,
    })
    .unwrap();

    let enforcer = Arc::new(Enforcer::new("EU_WEST", policy_cache, audit_log));
    let mut catalog = TenantCatalog::new();
    let mut classes = HashSet::new();
    classes.insert("sovereign-class".to_string());
    catalog.register_topic(TopicMetadata::new(
        "financial-transactions",
        "tenant-sovereign",
        3,
        classes,
    ));
    let broker = Arc::new(
        BrokerServer::try_new(
            "EU_WEST".to_string(),
            enforcer,
            meta.clone(),
            vec!["US_EAST".to_string()],
            dir.path(),
        )
        .unwrap()
        .with_catalog(catalog),
    );
    let _ = broker.claim_partition_ownership("financial-transactions", 0, 0, 1, 30_000);

    let state = GatewayState::new(broker);
    let app = app(state);
    let (session, csrf) = login_user(&app, "auditor", "kamish-auditor-2026").await;

    // A. Untouched file passes and reports 2 anchored, 1 unanchored tail
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
    assert_eq!(json["anchored_boundary"], 2);
    assert_eq!(json["unanchored_tail_count"], 1);
    assert_eq!(json["total_entries"], 3);

    // B. Tail removal beyond checkpoint reports reduced unanchored tail
    let raw_lines: Vec<String> = std::fs::read_to_string(&audit_file)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(raw_lines.len(), 3);
    // Write back only the first 2 lines (truncating unanchored entry 2)
    std::fs::write(&audit_file, format!("{}\n{}\n", raw_lines[0], raw_lines[1])).unwrap();

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
    assert_eq!(json["anchored_boundary"], 2);
    assert_eq!(json["unanchored_tail_count"], 0); // Tail removed beyond checkpoint reported as 0 unanchored!

    // C. Middle edit causes hash chain failure
    let tampered_line = raw_lines[0].replace("admission-grant-0", "TAMPERED-CONTENT");
    std::fs::write(
        &audit_file,
        format!("{}\n{}\n", tampered_line, raw_lines[1]),
    )
    .unwrap();

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
    assert_eq!(json["verification_result"], "FAIL");
    assert_eq!(json["hash_chain_valid"], false);
    assert_eq!(json["tamper_detected"], true);
}

#[tokio::test]
async fn test_security_headers_csp_hsts_and_cookies() {
    let (app, _dir) = setup_test_gateway();

    // 1. Regular HTTP request: CSP present, HSTS absent
    let req = Request::builder()
        .uri("/api/status")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let headers = res.headers();
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert!(
        headers
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("default-src 'self'")
    );
    assert!(headers.get("strict-transport-security").is_none());

    // 2. HTTPS request: HSTS present
    let req = Request::builder()
        .uri("/api/status")
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let headers = res.headers();
    assert!(headers.get("strict-transport-security").is_some());
    assert!(
        headers
            .get("strict-transport-security")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("max-age=31536000")
    );

    // 3. Login over HTTPS sets Secure cookie
    let login_req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header("x-forwarded-proto", "https")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"username":"admin","password":"kamish-admin-2026"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(login_req).await.unwrap();
    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("; Secure"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
}

#[tokio::test]
async fn test_tenant_isolation_and_rbac_denial() {
    let (app, _dir) = setup_test_gateway();

    // 1. Guest user from "tenant-guest" cannot produce to "financial-transactions" ("tenant-sovereign")
    let (guest_session, guest_csrf) = login_user(&app, "guest_producer", "kamish-guest-2026").await;
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/produce")
        .header(header::COOKIE, format!("kamish_session={guest_session}"))
        .header("x-csrf-token", &guest_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"topic":"financial-transactions","partition":0,"class_id":"sovereign-class","payload":"unauthorized-tenant-data"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = response_json(res).await;
    assert!(json["error"].as_str().unwrap().contains("Tenant mismatch"));

    // 2. Low-privilege Consumer is denied admin operations (403 Forbidden)
    let (consumer_session, consumer_csrf) =
        login_user(&app, "consumer", "kamish-consumer-2026").await;
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/policy/publish")
        .header(header::COOKIE, format!("kamish_session={consumer_session}"))
        .header("x-csrf-token", &consumer_csrf)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 3. Low-privilege Consumer is denied scenario execution (403 Forbidden)
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/scenarios/run")
        .header(header::COOKIE, format!("kamish_session={consumer_session}"))
        .header("x-csrf-token", &consumer_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"scenario_id":"residency"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_session_token_rotation() {
    let (app, _dir) = setup_test_gateway();

    // 1. Login user -> token 1
    let (token1, _csrf1) = login_user(&app, "admin", "kamish-admin-2026").await;

    // 2. Token 1 can access protected endpoint
    let req = Request::builder()
        .uri("/api/status")
        .header(header::COOKIE, format!("kamish_session={token1}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Login again passing existing session -> token rotates, old token invalidated
    let login_req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::COOKIE, format!("kamish_session={token1}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"username":"admin","password":"kamish-admin-2026"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = response_json(res).await;
    let token2 = json["session_token"].as_str().unwrap();
    assert_ne!(token1, token2);

    // 4. Token 1 is now invalidated -> 401 Unauthorized
    let req = Request::builder()
        .uri("/api/status")
        .header(header::COOKIE, format!("kamish_session={token1}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 5. Token 2 is valid -> 200 OK
    let req = Request::builder()
        .uri("/api/status")
        .header(header::COOKIE, format!("kamish_session={token2}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
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
