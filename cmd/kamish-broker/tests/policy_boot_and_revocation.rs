use kamish_policy::{
    KeyRevocationNotice, PolicyCache, PolicyDocument, PolicyError, SignedKeyRevocation,
    SignedPolicy, load_trusted_keys_from_file, load_verified_policy_from_file,
};
use rand::rngs::OsRng;
use std::collections::{HashMap, HashSet};

fn make_test_keys() -> (ed25519_dalek::SigningKey, ed25519_dalek::VerifyingKey) {
    let signing_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

#[test]
fn test_daemon_boot_fails_on_untrusted_or_tampered_policy() {
    let (signing_key, verifying_key) = make_test_keys();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());

    let (untrusted_key, _) = make_test_keys();

    let doc = PolicyDocument {
        policy_id: "pol-boot-test".to_string(),
        version: 1,
        issued_at_sec: 1000,
        ttl_seconds: 3600,
        issuer_pubkey_hex: pubkey_hex.clone(),
        classes: HashMap::new(),
        transit_rules: HashMap::new(),
    };

    let signed_valid = SignedPolicy::sign(doc.clone(), &signing_key);
    let signed_untrusted = SignedPolicy::sign(doc, &untrusted_key);

    let temp_dir = tempfile::tempdir().unwrap();
    let valid_policy_path = temp_dir.path().join("valid_policy.json");
    let untrusted_policy_path = temp_dir.path().join("untrusted_policy.json");
    let corrupted_policy_path = temp_dir.path().join("corrupted_policy.json");
    let keys_path = temp_dir.path().join("trusted_keys.json");

    std::fs::write(
        &valid_policy_path,
        serde_json::to_string(&signed_valid).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &untrusted_policy_path,
        serde_json::to_string(&signed_untrusted).unwrap(),
    )
    .unwrap();

    let mut corrupted = signed_valid.clone();
    corrupted.signature_hex = hex::encode(vec![0u8; 64]);
    std::fs::write(
        &corrupted_policy_path,
        serde_json::to_string(&corrupted).unwrap(),
    )
    .unwrap();

    std::fs::write(
        &keys_path,
        serde_json::to_string(&vec![pubkey_hex.clone()]).unwrap(),
    )
    .unwrap();

    let trusted_keys = load_trusted_keys_from_file(&keys_path).unwrap();

    // 1. Valid policy boots cleanly (W11)
    let loaded =
        load_verified_policy_from_file(&valid_policy_path, &trusted_keys, 1, 1500).unwrap();
    assert_eq!(loaded.policy.version, 1);

    // 2. Untrusted policy signature fails boot (fail-closed, W11)
    let err =
        load_verified_policy_from_file(&untrusted_policy_path, &trusted_keys, 1, 1500).unwrap_err();
    assert!(
        matches!(err, PolicyError::SignatureError(_)),
        "Expected signature error, got: {:?}",
        err
    );

    // 3. Corrupted cryptographic signature fails boot (fail-closed, W11)
    let err =
        load_verified_policy_from_file(&corrupted_policy_path, &trusted_keys, 1, 1500).unwrap_err();
    assert!(
        matches!(err, PolicyError::SignatureError(_)),
        "Expected signature error, got: {:?}",
        err
    );
}

#[test]
fn test_durable_policy_version_floor_persistence() {
    let (signing_key, verifying_key) = make_test_keys();
    let pubkey_hex = hex::encode(verifying_key.to_bytes());
    let trusted_keys = HashSet::from([pubkey_hex.clone()]);

    let temp_dir = tempfile::tempdir().unwrap();
    let floor_file = temp_dir.path().join("policy_floor.txt");
    let policy_path = temp_dir.path().join("policy.json");

    // Persist a durable floor of 10
    std::fs::write(&floor_file, "10").unwrap();

    let stored_floor: u64 = std::fs::read_to_string(&floor_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(stored_floor, 10);

    // Policy with version 8 must fail against durable floor 10 (W11)
    let doc_v8 = PolicyDocument {
        policy_id: "pol-boot".to_string(),
        version: 8,
        issued_at_sec: 1000,
        ttl_seconds: 3600,
        issuer_pubkey_hex: pubkey_hex.clone(),
        classes: HashMap::new(),
        transit_rules: HashMap::new(),
    };
    let signed_v8 = SignedPolicy::sign(doc_v8, &signing_key);
    std::fs::write(&policy_path, serde_json::to_string(&signed_v8).unwrap()).unwrap();

    let err = load_verified_policy_from_file(&policy_path, &trusted_keys, stored_floor, 1100)
        .unwrap_err();
    assert!(matches!(
        err,
        PolicyError::VersionBelowFloor {
            floor: 10,
            received: 8
        }
    ));

    // Policy with version 12 succeeds and advances durable floor
    let doc_v12 = PolicyDocument {
        policy_id: "pol-boot".to_string(),
        version: 12,
        issued_at_sec: 1000,
        ttl_seconds: 3600,
        issuer_pubkey_hex: pubkey_hex,
        classes: HashMap::new(),
        transit_rules: HashMap::new(),
    };
    let signed_v12 = SignedPolicy::sign(doc_v12, &signing_key);
    std::fs::write(&policy_path, serde_json::to_string(&signed_v12).unwrap()).unwrap();

    let loaded =
        load_verified_policy_from_file(&policy_path, &trusted_keys, stored_floor, 1100).unwrap();
    assert_eq!(loaded.policy.version, 12);

    // Update persisted floor file
    std::fs::write(&floor_file, loaded.policy.version.to_string()).unwrap();
    let updated_floor: u64 = std::fs::read_to_string(&floor_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(updated_floor, 12);
}

#[test]
fn test_key_revocation_and_rotation_protocol_full() {
    let (root_sk, root_vk) = make_test_keys();
    let root_pubkey_hex = hex::encode(root_vk.to_bytes());

    let (key_a_sk, key_a_vk) = make_test_keys();
    let key_a_pubkey_hex = hex::encode(key_a_vk.to_bytes());

    let (key_b_sk, key_b_vk) = make_test_keys();
    let key_b_pubkey_hex = hex::encode(key_b_vk.to_bytes());

    let mut cache = PolicyCache::new(HashSet::from([key_a_pubkey_hex.clone()]))
        .with_authorizers(HashSet::from([root_pubkey_hex.clone()]));

    // 1. Install policy signed by Key A
    let doc_a = PolicyDocument {
        policy_id: "pol-key-test".to_string(),
        version: 1,
        issued_at_sec: 1000,
        ttl_seconds: 3600,
        issuer_pubkey_hex: key_a_pubkey_hex.clone(),
        classes: HashMap::new(),
        transit_rules: HashMap::new(),
    };
    let signed_a = SignedPolicy::sign(doc_a, &key_a_sk);
    cache.update(signed_a, 1100).unwrap();
    assert!(cache.current().is_some());

    // 2. Authorizer signs revocation for Key A (W12)
    let notice = KeyRevocationNotice::new(
        &key_a_pubkey_hex,
        "Emergency security revocation of compromised Key A",
        1200,
        &root_pubkey_hex,
    );
    let signed_revocation = SignedKeyRevocation::sign(notice, &root_sk);

    // 3. Rotate key: install Key B and revoke Key A
    cache
        .rotate_key(key_b_pubkey_hex.clone(), Some(signed_revocation), 1200)
        .unwrap();

    assert!(cache.is_key_revoked(&key_a_pubkey_hex));
    assert!(
        cache.current().is_none(),
        "Active policy signed by revoked key must be evicted"
    );

    // 4. Any attempt to use revoked Key A is rejected
    let doc_a_new = PolicyDocument {
        policy_id: "pol-key-test".to_string(),
        version: 2,
        issued_at_sec: 1250,
        ttl_seconds: 3600,
        issuer_pubkey_hex: key_a_pubkey_hex.clone(),
        classes: HashMap::new(),
        transit_rules: HashMap::new(),
    };
    let signed_a_new = SignedPolicy::sign(doc_a_new, &key_a_sk);
    let err = cache.update(signed_a_new, 1300).unwrap_err();
    assert!(matches!(err, PolicyError::KeyRevoked(_)));

    // 5. New rotated Key B can successfully sign and install policy
    let doc_b = PolicyDocument {
        policy_id: "pol-key-test".to_string(),
        version: 2,
        issued_at_sec: 1250,
        ttl_seconds: 3600,
        issuer_pubkey_hex: key_b_pubkey_hex,
        classes: HashMap::new(),
        transit_rules: HashMap::new(),
    };
    let signed_b = SignedPolicy::sign(doc_b, &key_b_sk);
    assert!(cache.update(signed_b, 1300).is_ok());
    assert_eq!(cache.current().unwrap().policy.version, 2);
}
