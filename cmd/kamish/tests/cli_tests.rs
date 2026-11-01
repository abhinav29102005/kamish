use axum::http::Uri;
use kamish::assets::{EmbeddedAssets, static_handler};

#[tokio::test]
async fn test_embedded_assets_inventory() {
    let count = EmbeddedAssets::iter().count();
    assert!(
        count >= 10,
        "Expected at least 10 embedded assets, found {count}"
    );
    assert!(
        EmbeddedAssets::get("index.html").is_some(),
        "index.html must be embedded"
    );
    assert!(
        EmbeddedAssets::get("favicon.svg").is_some(),
        "favicon.svg must be embedded"
    );
}

#[tokio::test]
async fn test_static_handler_index_and_caching() {
    let uri: Uri = "/".parse().unwrap();
    let res = static_handler(uri).await;
    assert_eq!(res.status(), axum::http::StatusCode::OK);

    let headers = res.headers();
    let content_type = headers.get("content-type").unwrap().to_str().unwrap();
    assert!(
        content_type.starts_with("text/html"),
        "content-type must start with text/html, got {content_type}"
    );
    assert_eq!(
        headers.get("cache-control").unwrap(),
        "no-cache, no-store, must-revalidate"
    );
    assert!(headers.get("content-security-policy").is_some());
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
}

#[tokio::test]
async fn test_static_handler_spa_routing_fallback() {
    // Non-file route should serve index.html with SPA fallback
    let uri: Uri = "/dashboard".parse().unwrap();
    let res = static_handler(uri).await;
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.starts_with("text/html"));

    // Missing file with extension should 404
    let uri_missing: Uri = "/assets/non-existent-chunk.js".parse().unwrap();
    let res_missing = static_handler(uri_missing).await;
    assert_eq!(res_missing.status(), axum::http::StatusCode::NOT_FOUND);
}

#[test]
fn test_doctor_diagnostic_run() {
    let code = kamish::doctor::run_doctor(None, true);
    // Doctor runs and returns 0 or 1 depending on whether etcd is online, but must never panic
    assert!(code == 0 || code == 1);
}

#[test]
fn test_init_and_idempotence_guard() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir_str = temp_dir.path().to_str().unwrap();

    let opts = kamish::init::InitOptions {
        dir: dir_str,
        secret_file: None,
        admin_user: "test-admin",
        admin_password: Some("secret123"),
        region: "EU_WEST",
        force: false,
    };

    let code = kamish::init::run_init(opts);
    assert_eq!(code, 0, "First init must succeed");

    assert!(temp_dir.path().join("kamish.toml").exists());
    assert!(temp_dir.path().join("users.txt").exists());
    assert!(temp_dir.path().join("admin.key").exists());
    assert!(temp_dir.path().join("admin.pub").exists());
    assert!(temp_dir.path().join("policy.signed.json").exists());

    // Without force, second init must refuse to overwrite
    let opts_second = kamish::init::InitOptions {
        dir: dir_str,
        secret_file: None,
        admin_user: "test-admin",
        admin_password: Some("secret123"),
        region: "EU_WEST",
        force: false,
    };
    let code_second = kamish::init::run_init(opts_second);
    assert_eq!(code_second, 1, "Second init without --force must fail");

    // With force, second init must succeed
    let opts_force = kamish::init::InitOptions {
        dir: dir_str,
        secret_file: None,
        admin_user: "test-admin",
        admin_password: Some("secret123"),
        region: "EU_WEST",
        force: true,
    };
    let code_force = kamish::init::run_init(opts_force);
    assert_eq!(code_force, 0, "Init with --force must succeed");
}

#[test]
fn test_policy_sign_and_verify_flow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir_str = temp_dir.path().to_str().unwrap();

    // Init to get key and pubkey
    let opts = kamish::init::InitOptions {
        dir: dir_str,
        secret_file: None,
        admin_user: "admin",
        admin_password: Some("pass"),
        region: "EU_WEST",
        force: true,
    };
    assert_eq!(kamish::init::run_init(opts), 0);

    let _key_file = temp_dir.path().join("admin.key");
    let pub_file = temp_dir.path().join("admin.pub");
    let policy_file = temp_dir.path().join("policy.signed.json");

    let verify_code = kamish::policy_cmd::verify_policy(
        policy_file.to_str().unwrap(),
        pub_file.to_str().unwrap(),
    );
    assert_eq!(verify_code, 0, "Policy verification must pass");
}
