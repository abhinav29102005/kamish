use chrono::Utc;
use ed25519_dalek::SigningKey;
use kamish_audit::{AuditAction, AuditLog};
use kamish_enforce::{EnforcementRequest, Enforcer};
use kamish_policy::{
    AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
};
use kamish_seal::{RegionalKeyService, SealEngine};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

fn create_keypair() -> (SigningKey, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pubkey_hex = hex::encode(signing_key.verifying_key().to_bytes());
    (signing_key, pubkey_hex)
}

fn build_signed_policy(
    signing_key: &SigningKey,
    pubkey_hex: &str,
    issued_offset_sec: i64,
    ttl_seconds: u64,
) -> SignedPolicy {
    let mut legal_eu = HashSet::new();
    legal_eu.insert("region-eu".to_string());

    let mut modes_eu = HashMap::new();
    modes_eu.insert("region-eu".to_string(), DestinationMode::Cleartext);

    let mut classes = HashMap::new();
    classes.insert(
        "strict-sovereign".to_string(),
        ClassRule {
            class_id: "strict-sovereign".to_string(),
            description: "Strict local EU data only".to_string(),
            legal_regions: legal_eu,
            destination_modes: modes_eu,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-eu".to_string()],
            transit_allowed: false,
        },
    );

    let doc = PolicyDocument {
        policy_id: "redteam-pol".to_string(),
        version: 1,
        issued_at_sec: Utc::now().timestamp() - issued_offset_sec,
        ttl_seconds,
        issuer_pubkey_hex: pubkey_hex.to_string(),
        classes,
        transit_rules: HashMap::new(),
    };

    SignedPolicy::sign(doc, signing_key)
}

#[test]
fn test_invalid_signature_and_tampered_policy_rejection() {
    let (signing_key, pubkey_hex) = create_keypair();
    let mut signed = build_signed_policy(&signing_key, &pubkey_hex, 10, 3600);

    // Verify valid signature initially
    assert!(signed.verify_signature().is_ok());

    // 1. Tamper with policy payload content (e.g. inject an unauthorized region into legal_regions)
    signed
        .policy
        .classes
        .get_mut("strict-sovereign")
        .unwrap()
        .legal_regions
        .insert("unauthorized-region".to_string());

    // 2. Signature verification MUST fail
    let tamper_res = signed.verify_signature();
    assert!(
        tamper_res.is_err(),
        "Tampered policy document must fail cryptographic signature verification"
    );

    // 3. Tamper with signature bytes directly
    let mut signed2 = build_signed_policy(&signing_key, &pubkey_hex, 10, 3600);
    signed2.signature_hex = hex::encode([0xDE, 0xAD, 0xBE, 0xEF]);
    assert!(signed2.verify_signature().is_err());
}

#[test]
fn test_missing_policy_denies_all_operations_closed() {
    let (_, pubkey_hex) = create_keypair();
    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex);

    // Empty policy cache (no policy loaded)
    let cache = Arc::new(RwLock::new(PolicyCache::new(trusted)));
    let audit = AuditLog::new_in_memory();
    let enforcer = Enforcer::new("region-eu", cache, audit);

    let req = EnforcementRequest::new(
        AuditAction::Ingress,
        "strict-sovereign",
        "region-eu",
        "region-eu",
    );
    let decision = enforcer.decide(req);

    // Must fail closed
    assert!(!decision.allowed, "Missing policy must fail closed");
    assert_eq!(decision.reason, "no_policy_loaded");
    assert_eq!(decision.mode, DestinationMode::Deny);
}

#[test]
fn test_expired_policy_denies_operations() {
    let (signing_key, pubkey_hex) = create_keypair();
    // 1. Policy that is already expired cannot even be installed into PolicyCache
    let expired_policy = build_signed_policy(&signing_key, &pubkey_hex, 500, 10);

    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex.clone());

    let mut cache = PolicyCache::new(trusted.clone());
    let update_err = cache.update(expired_policy, Utc::now().timestamp());
    assert!(
        update_err.is_err(),
        "Installing an already expired policy must be rejected with PolicyStale"
    );

    // 2. Policy installed when fresh, but expires over time: enforcer must reject operations
    let short_lived_policy = build_signed_policy(&signing_key, &pubkey_hex, 0, 1);
    let mut cache2 = PolicyCache::new(trusted);
    cache2
        .update(short_lived_policy, Utc::now().timestamp())
        .unwrap();
    let cache_arc = Arc::new(RwLock::new(cache2));

    let audit = AuditLog::new_in_memory();
    let enforcer = Enforcer::new("region-eu", cache_arc, audit);

    // Sleep 2.1s so the 1-second TTL definitely rolls over the epoch boundary
    std::thread::sleep(std::time::Duration::from_millis(2100));

    let req = EnforcementRequest::new(
        AuditAction::Ingress,
        "strict-sovereign",
        "region-eu",
        "region-eu",
    );
    let decision = enforcer.decide(req);

    assert!(!decision.allowed, "Expired policy must be rejected");
    assert_eq!(decision.reason, "stale_policy_expired");
    assert_eq!(decision.mode, DestinationMode::Deny);
}

#[test]
fn test_unauthorized_destination_egress_denial() {
    let (signing_key, pubkey_hex) = create_keypair();
    let policy = build_signed_policy(&signing_key, &pubkey_hex, 10, 3600);

    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex);

    let mut cache = PolicyCache::new(trusted);
    cache.update(policy, Utc::now().timestamp()).unwrap();
    let cache_arc = Arc::new(RwLock::new(cache));

    let audit = AuditLog::new_in_memory();
    let enforcer = Enforcer::new("region-eu", cache_arc, audit.clone());

    // Ingress in EU is allowed
    let ing_req = EnforcementRequest::new(
        AuditAction::Ingress,
        "strict-sovereign",
        "region-eu",
        "region-eu",
    );
    assert!(enforcer.decide(ing_req).allowed);

    // Replication to US is strictly forbidden
    let rep_req = EnforcementRequest::new(
        AuditAction::ReplicationSender,
        "strict-sovereign",
        "region-eu",
        "region-us",
    );
    let rep_decision = enforcer.decide(rep_req);
    assert!(!rep_decision.allowed);
    assert_eq!(rep_decision.mode, DestinationMode::Deny);
    assert_eq!(rep_decision.reason, "target_not_in_legal_set");

    // Verify Audit Log records the denial with unbroken hash chain
    let entries = audit.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].action, AuditAction::ReplicationSender);
    assert_eq!(entries[1].outcome, kamish_audit::DecisionOutcome::Deny);
    assert!(audit.verify_integrity().is_ok());
}

#[test]
fn test_audit_ledger_zero_payload_disclosure() {
    let (signing_key, pubkey_hex) = create_keypair();
    let policy = build_signed_policy(&signing_key, &pubkey_hex, 10, 3600);

    let mut trusted = HashSet::new();
    trusted.insert(pubkey_hex);

    let mut cache = PolicyCache::new(trusted);
    cache.update(policy, Utc::now().timestamp()).unwrap();
    let cache_arc = Arc::new(RwLock::new(cache));

    let audit = AuditLog::new_in_memory();
    let enforcer = Enforcer::new("region-eu", cache_arc, audit.clone());

    let req = EnforcementRequest::new(
        AuditAction::Ingress,
        "strict-sovereign",
        "region-eu",
        "region-eu",
    );
    enforcer.decide(req);

    // Inspect serialized JSON of audit entry
    let entries = audit.entries();
    let entry_json = serde_json::to_string(&entries[0]).unwrap();

    // Verify audit entry JSON does NOT contain any payload field
    assert!(!entry_json.contains("payload"));
    assert!(!entry_json.contains("data"));
    assert!(!entry_json.contains("message_body"));
}

#[test]
fn test_cross_border_key_custody_denial() {
    let key_service = RegionalKeyService::new("region-eu");
    key_service.provision_key("eu-key-01", 1, [0x42u8; 32]);

    let secret_payload = b"top-secret-gdpr-pii-data";

    // 1. Seal payload within region-eu
    let sealed = SealEngine::transform(
        secret_payload,
        DestinationMode::SealedOnly,
        "eu-key-01",
        "region-eu",
        &key_service,
    )
    .unwrap();

    // 2. Unseal within authorized region-eu succeeds
    let unsealed = SealEngine::unseal(&sealed, "region-eu", &key_service).unwrap();
    assert_eq!(unsealed, secret_payload);

    // 3. Unseal attempt from unauthorized region-us is DENIED by RegionalKeyService
    let denied = SealEngine::unseal(&sealed, "region-us", &key_service);
    assert!(denied.is_err());
    assert!(matches!(
        denied.unwrap_err(),
        kamish_seal::SealError::CrossBorderKeyAccessDenied { .. }
    ));
}
