use ed25519_dalek::SigningKey;
use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, PolicyError, SignedPolicy};
use std::collections::HashMap;

fn fixture() -> (SigningKey, PolicyDocument) {
    let key = SigningKey::from_bytes(&[17; 32]);
    let classes = (0..32).map(|i| {
        let id = format!("class-{i}");
        (id.clone(), ClassRule {
            class_id: id, description: "test".into(),
            legal_regions: ["eu".into(), "us".into(), "ap".into()].into(),
            destination_modes: [("eu".into(), DestinationMode::Cleartext),
                ("us".into(), DestinationMode::SealedOnly)].into(),
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["eu".into(), "us".into()],
            transit_allowed: true,
        })
    }).collect();
    let doc = PolicyDocument {
        policy_id: "test".into(), version: u64::MAX,
        issued_at_sec: 1000, ttl_seconds: 100,
        issuer_pubkey_hex: hex::encode(key.verifying_key().to_bytes()), classes,
        transit_rules: [("eu".into(), vec!["us".into(), "ap".into()])].into(),
    };
    (key, doc)
}

#[test]
fn signature_survives_repeated_wire_roundtrips() {
    let (key, doc) = fixture();
    let original = SignedPolicy::sign(doc, &key);
    let bytes = serde_json::to_vec(&original).unwrap();
    for _ in 0..32 {
        let decoded: SignedPolicy = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(original.policy, decoded.policy);
        decoded.verify_signature().unwrap();
    }
}

#[test]
fn map_and_set_order_is_irrelevant_but_preference_order_is_signed() {
    let (key, doc) = fixture();
    let signed = SignedPolicy::sign(doc, &key);
    let mut reordered = signed.clone();
    reordered.policy.classes = signed.policy.classes.iter().map(|(id, rule)| {
        let mut rule = rule.clone();
        rule.legal_regions = rule.legal_regions.iter().cloned().collect();
        rule.destination_modes = rule.destination_modes.iter().map(|(r, m)| (r.clone(), *m)).collect();
        (id.clone(), rule)
    }).collect::<HashMap<_, _>>();
    reordered.policy.transit_rules.get_mut("eu").unwrap().reverse();
    reordered.verify_signature().unwrap();
    reordered.policy.classes.get_mut("class-0").unwrap().failover_candidates.reverse();
    assert!(reordered.verify_signature().is_err());
}

#[test]
fn changed_same_version_and_policy_identity_are_rejected() {
    let (key, doc) = fixture();
    let mut cache = PolicyCache::new([doc.issuer_pubkey_hex.clone()].into());
    let original = SignedPolicy::sign(doc.clone(), &key);
    cache.update(original.clone(), 1050).unwrap();
    cache.update(original, 1050).unwrap();
    let mut changed = doc.clone();
    changed.classes.clear();
    assert!(matches!(cache.update(SignedPolicy::sign(changed, &key), 1050), Err(PolicyError::VersionConflict(_))));
    let mut changed = doc;
    changed.policy_id = "different-authority-scope".into();
    assert!(matches!(cache.update(SignedPolicy::sign(changed, &key), 1050), Err(PolicyError::IdentityChanged)));
    assert_eq!(cache.current().unwrap().policy.classes.len(), 32);
}

#[test]
fn expiry_is_exclusive_and_overflow_fails_closed() {
    let (key, doc) = fixture();
    let mut signed = SignedPolicy::sign(doc, &key);
    assert!(!signed.is_fresh(999));
    assert!(signed.is_fresh(1000));
    assert!(signed.is_fresh(1099));
    assert!(!signed.is_fresh(1100));
    signed.policy.ttl_seconds = u64::MAX;
    assert!(!signed.is_fresh(1050));
    signed.policy.ttl_seconds = 100;
    signed.policy.issued_at_sec = i64::MAX - 10;
    assert!(!signed.is_fresh(i64::MAX - 5));
}

#[test]
fn legacy_or_unknown_signature_formats_are_not_silently_accepted() {
    let (key, doc) = fixture();
    let signed = SignedPolicy::sign(doc, &key);
    let mut json = serde_json::to_value(&signed).unwrap();
    json.as_object_mut().unwrap().remove("signature_format");
    let legacy: SignedPolicy = serde_json::from_value(json).unwrap();
    assert!(matches!(legacy.verify_signature(), Err(PolicyError::UnsupportedFormat(0))));
    let mut future = signed;
    future.signature_format = 999;
    assert!(matches!(future.verify_signature(), Err(PolicyError::UnsupportedFormat(999))));
}
