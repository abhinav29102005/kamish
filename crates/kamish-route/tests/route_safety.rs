use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::AuditLog;
use kamish_enforce::Enforcer;
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use kamish_route::{LinkMetrics, PathCandidate, RouteSelector};
use std::sync::{Arc, RwLock};

fn fixture() -> (RouteSelector, Arc<RwLock<PolicyCache>>, SigningKey) {
    let key = SigningKey::from_bytes(&[23; 32]);
    let class = ClassRule {
        class_id: "open".into(),
        description: String::new(),
        legal_regions: ["a".into(), "b".into(), "c".into()].into(),
        destination_modes: [
            ("a".into(), DestinationMode::Cleartext),
            ("b".into(), DestinationMode::Cleartext),
            ("c".into(), DestinationMode::Cleartext),
        ]
        .into(),
        ack_mode: AckMode::LocalMajority,
        failover_candidates: vec!["a".into()],
        transit_allowed: true,
    };
    let mut restricted = class.clone();
    restricted.class_id = "restricted".into();
    restricted.transit_allowed = false;
    let doc = PolicyDocument {
        policy_id: "routes".into(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - 1,
        ttl_seconds: 3600,
        issuer_pubkey_hex: hex::encode(key.verifying_key().to_bytes()),
        classes: [("open".into(), class), ("restricted".into(), restricted)].into(),
        transit_rules: [("a".into(), vec!["relay".into()])].into(),
    };
    let mut cache = PolicyCache::new([doc.issuer_pubkey_hex.clone()].into());
    cache
        .update(SignedPolicy::sign(doc, &key), Utc::now().timestamp())
        .unwrap();
    let cache = Arc::new(RwLock::new(cache));
    let enforcer = Enforcer::new("a", cache.clone(), AuditLog::new_in_memory());
    (RouteSelector::new("a", enforcer), cache, key)
}

fn path(relay: Option<&str>, rtt: f64) -> PathCandidate {
    PathCandidate {
        target_region: "b".into(),
        relay_region: relay.map(str::to_string),
        metrics: LinkMetrics {
            rtt_ms: rtt,
            ..Default::default()
        },
    }
}

#[test]
fn revocation_bypasses_dwell_and_revalidates_cached_route() {
    let (router, cache, key) = fixture();
    let paths = [path(Some("relay"), 1.0), path(None, 100.0)];
    assert_eq!(
        router
            .pick_path_for_payload("b", "open", &paths, true)
            .unwrap()
            .relay_region
            .as_deref(),
        Some("relay")
    );
    let mut doc = cache.read().unwrap().current().unwrap().policy.clone();
    doc.version += 1;
    doc.transit_rules.clear();
    cache
        .write()
        .unwrap()
        .update(SignedPolicy::sign(doc, &key), Utc::now().timestamp())
        .unwrap();
    assert_eq!(
        router
            .pick_path_for_payload("b", "open", &paths, true)
            .unwrap()
            .relay_region,
        None
    );
}

#[test]
fn class_caches_are_isolated_and_plaintext_never_gets_a_relay() {
    let (router, _, _) = fixture();
    let paths = [path(Some("relay"), 1.0), path(None, 100.0)];
    assert!(
        router
            .pick_path_for_payload("b", "open", &paths, true)
            .unwrap()
            .relay_region
            .is_some()
    );
    assert!(
        router
            .pick_path_for_payload("b", "restricted", &paths, true)
            .unwrap()
            .relay_region
            .is_none()
    );
    assert!(
        router
            .pick_path("b", "open", &paths)
            .unwrap()
            .relay_region
            .is_none()
    );
    assert!(router.pick_path("b", "open", &paths[..1]).is_none());
}

#[test]
fn unknown_relays_and_wrong_destinations_are_rejected() {
    let (router, _, _) = fixture();
    assert!(
        router
            .pick_path_for_payload("b", "open", &[path(Some("unlisted"), 1.0)], true)
            .is_none()
    );
    let mut wrong = path(None, 1.0);
    wrong.target_region = "c".into();
    assert!(router.pick_path("b", "open", &[wrong]).is_none());
}

#[test]
fn retained_route_uses_current_measurements() {
    let (router, _, _) = fixture();
    router.pick_path("b", "open", &[path(None, 1.0)]).unwrap();
    let result = router.pick_path("b", "open", &[path(None, 500.0)]).unwrap();
    assert_eq!(result.metrics.rtt_ms, 500.0);
    assert!(router.pick_path("b", "open", &[]).is_none());
}

#[test]
fn invalid_measurements_cannot_win_or_retain_a_route() {
    let (router, _, _) = fixture();
    router.pick_path("b", "open", &[path(None, 1.0)]).unwrap();
    for rtt in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(router.pick_path("b", "open", &[path(None, rtt)]).is_none());
    }
    let mut invalid = path(None, 1.0);
    invalid.metrics.loss_rate = 1.1;
    assert!(router.pick_path("b", "open", &[invalid]).is_none());
}
