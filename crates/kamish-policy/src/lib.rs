//! Sovereignty Policy Data Model, Verification, and Versioning for Hydra Stream.
//!
//! Complies with `project.md`:
//! - §2.4: Policy Data Model (Class rules, legal sets, destination modes, transit rules, failover candidates).
//! - §3.3.3: Policy Freshness and Versioning State Machine.
//! - INV-9: An unsigned, invalid, or rolled-back policy is never applied.
//! - FR-01: Reject operations where data class is not permitted in target region.
//! - FR-08: Promote a new owner only if policy lists it as eligible.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Cryptographic signature error: {0}")]
    SignatureError(String),

    #[error("Unsupported policy signature format {0}; re-sign with format 2")]
    UnsupportedFormat(u32),

    #[error("Policy identity changed without an explicit migration")]
    IdentityChanged,

    #[error("Policy version {0} was reused for different content")]
    VersionConflict(u64),

    #[error("Policy is stale: current time {now_sec} exceeds expiry {expiry_sec}")]
    PolicyStale { now_sec: i64, expiry_sec: i64 },

    #[error(
        "Policy version rollback rejected: current cached version is {cached}, received {received}"
    )]
    VersionRollback { cached: u64, received: u64 },

    #[error("Policy version {received} is below durable floor {floor}")]
    VersionBelowFloor { floor: u64, received: u64 },

    #[error("Key '{0}' has been revoked and cannot sign policies")]
    KeyRevoked(String),

    #[error("Authorizer key '{0}' is not in trusted authorizers list")]
    UntrustedAuthorizer(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Class '{0}' not found in policy")]
    ClassNotFound(String),
}

/// Destination storage / transmission mode (§1.6 D2, §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DestinationMode {
    Cleartext,
    SealedOnly,
    Deny,
}

/// Acknowledgment durability level (§2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AckMode {
    LocalMajority,
    RegionMajority,
    QuorumAll,
}

/// Rules governing an individual data classification (§2.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassRule {
    pub class_id: String,
    pub description: String,
    pub legal_regions: HashSet<String>,
    pub destination_modes: HashMap<String, DestinationMode>,
    pub ack_mode: AckMode,
    pub failover_candidates: Vec<String>,
    pub transit_allowed: bool,
}

impl ClassRule {
    /// Checks if a region is legally permitted to store or process this class.
    pub fn is_region_legal(&self, region: &str) -> bool {
        self.legal_regions.contains(region)
    }

    /// Resolves the destination mode for a target region (defaults to Deny if not found).
    pub fn mode_for_region(&self, region: &str) -> DestinationMode {
        if !self.is_region_legal(region) {
            return DestinationMode::Deny;
        }
        self.destination_modes
            .get(region)
            .copied()
            .unwrap_or(DestinationMode::Deny)
    }

    /// Checks if a region is eligible for ownership failover promotion (FR-08).
    pub fn is_failover_eligible(&self, region: &str) -> bool {
        self.failover_candidates.iter().any(|r| r == region)
    }
}

/// Complete signed policy package (§2.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDocument {
    pub policy_id: String,
    pub version: u64,
    pub issued_at_sec: i64,
    pub ttl_seconds: u64,
    pub issuer_pubkey_hex: String,
    pub classes: HashMap<String, ClassRule>,
    pub transit_rules: HashMap<String, Vec<String>>, // Source -> allowed intermediate relay regions
}

impl PolicyDocument {
    /// Format 2: typed JSON with fixed struct fields, sorted maps and sorted sets.
    /// Ordered lists (including failover preference) retain their order. This is
    /// a versioned Kamish encoding, not RFC 8785/JCS; u64 values remain exact.
    pub fn canonical_digest(&self) -> Vec<u8> {
        #[derive(Serialize)]
        struct CanonicalClass<'a> {
            class_id: &'a str,
            description: &'a str,
            legal_regions: BTreeSet<&'a str>,
            destination_modes: BTreeMap<&'a str, DestinationMode>,
            ack_mode: AckMode,
            failover_candidates: &'a [String],
            transit_allowed: bool,
        }
        let classes: BTreeMap<_, _> = self
            .classes
            .iter()
            .map(|(id, rule)| {
                (
                    id.as_str(),
                    CanonicalClass {
                        class_id: &rule.class_id,
                        description: &rule.description,
                        legal_regions: rule.legal_regions.iter().map(String::as_str).collect(),
                        destination_modes: rule
                            .destination_modes
                            .iter()
                            .map(|(r, m)| (r.as_str(), *m))
                            .collect(),
                        ack_mode: rule.ack_mode,
                        failover_candidates: &rule.failover_candidates,
                        transit_allowed: rule.transit_allowed,
                    },
                )
            })
            .collect();
        let transit: BTreeMap<_, BTreeSet<_>> = self
            .transit_rules
            .iter()
            .map(|(source, regions)| {
                (
                    source.as_str(),
                    regions.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        let serialized = serde_json::to_vec(&(
            &self.policy_id,
            self.version,
            self.issued_at_sec,
            self.ttl_seconds,
            &self.issuer_pubkey_hex,
            classes,
            transit,
        ))
        .expect("policy contains only serializable primitive values");
        let mut hasher = Sha256::new();
        hasher.update(b"kamish-policy-signature-v2\0");
        hasher.update(&serialized);
        hasher.finalize().to_vec()
    }
}

/// Enveloped policy including signature for wire transfer and storage (INV-9).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedPolicy {
    #[serde(default)]
    pub signature_format: u32,
    pub policy: PolicyDocument,
    pub signature_hex: String,
}

impl SignedPolicy {
    /// Signs a policy document with an Ed25519 private key.
    pub fn sign(policy: PolicyDocument, signing_key: &SigningKey) -> Self {
        let digest = policy.canonical_digest();
        let signature = signing_key.sign(&digest);
        Self {
            signature_format: 2,
            policy,
            signature_hex: hex::encode(signature.to_bytes()),
        }
    }

    /// Verifies the signature against the issuer public key declared in the policy (INV-9).
    pub fn verify_signature(&self) -> Result<(), PolicyError> {
        if self.signature_format != 2 {
            return Err(PolicyError::UnsupportedFormat(self.signature_format));
        }
        let pubkey_bytes = hex::decode(&self.policy.issuer_pubkey_hex)
            .map_err(|e| PolicyError::SignatureError(format!("Invalid pubkey hex: {}", e)))?;
        let pubkey_arr: [u8; 32] = pubkey_bytes
            .try_into()
            .map_err(|_| PolicyError::SignatureError("Public key must be 32 bytes".into()))?;
        let verifying_key = VerifyingKey::from_bytes(&pubkey_arr)
            .map_err(|e| PolicyError::SignatureError(format!("Invalid verifying key: {}", e)))?;

        let sig_bytes = hex::decode(&self.signature_hex)
            .map_err(|e| PolicyError::SignatureError(format!("Invalid signature hex: {}", e)))?;
        let sig_arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| PolicyError::SignatureError("Signature must be 64 bytes".into()))?;
        let signature = Signature::from_bytes(&sig_arr);

        let digest = self.policy.canonical_digest();
        verifying_key.verify(&digest, &signature).map_err(|e| {
            PolicyError::SignatureError(format!("Signature verification failed: {}", e))
        })
    }

    /// Checks if the policy is currently within its validity time window.
    pub fn is_fresh(&self, now_sec: i64) -> bool {
        self.expires_at().is_some_and(|expiry| {
            self.policy.ttl_seconds > 0 && now_sec >= self.policy.issued_at_sec && now_sec < expiry
        })
    }

    fn expires_at(&self) -> Option<i64> {
        self.policy
            .issued_at_sec
            .checked_add(i64::try_from(self.policy.ttl_seconds).ok()?)
    }
}

/// Notice declaring cryptographic revocation of a signing key (W12).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyRevocationNotice {
    pub format_version: u32,
    pub revoked_key_hex: String,
    pub reason: String,
    pub revoked_at_sec: i64,
    pub authorizer_pubkey_hex: String,
}

impl KeyRevocationNotice {
    pub fn new(
        revoked_key_hex: impl Into<String>,
        reason: impl Into<String>,
        revoked_at_sec: i64,
        authorizer_pubkey_hex: impl Into<String>,
    ) -> Self {
        Self {
            format_version: 1,
            revoked_key_hex: revoked_key_hex.into(),
            reason: reason.into(),
            revoked_at_sec,
            authorizer_pubkey_hex: authorizer_pubkey_hex.into(),
        }
    }

    pub fn canonical_digest(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(b"kamish-key-revocation-v1\n");
        hasher.update(self.format_version.to_be_bytes());
        hasher.update(self.revoked_key_hex.as_bytes());
        hasher.update(self.reason.as_bytes());
        hasher.update(self.revoked_at_sec.to_be_bytes());
        hasher.update(self.authorizer_pubkey_hex.as_bytes());
        hasher.finalize().to_vec()
    }
}

/// Cryptographically signed key revocation statement (W12).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedKeyRevocation {
    pub notice: KeyRevocationNotice,
    pub signature_hex: String,
}

impl SignedKeyRevocation {
    pub fn sign(notice: KeyRevocationNotice, signing_key: &SigningKey) -> Self {
        let digest = notice.canonical_digest();
        let sig = signing_key.sign(&digest);
        Self {
            notice,
            signature_hex: hex::encode(sig.to_bytes()),
        }
    }

    pub fn verify_signature(&self) -> Result<(), PolicyError> {
        let pubkey_bytes = hex::decode(&self.notice.authorizer_pubkey_hex).map_err(|e| {
            PolicyError::SignatureError(format!("Invalid authorizer pubkey hex: {}", e))
        })?;
        let pubkey_arr: [u8; 32] = pubkey_bytes
            .try_into()
            .map_err(|_| PolicyError::SignatureError("Authorizer key must be 32 bytes".into()))?;
        let verifying_key = VerifyingKey::from_bytes(&pubkey_arr)
            .map_err(|e| PolicyError::SignatureError(format!("Invalid verifying key: {}", e)))?;

        let sig_bytes = hex::decode(&self.signature_hex)
            .map_err(|e| PolicyError::SignatureError(format!("Invalid signature hex: {}", e)))?;
        let sig_arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| PolicyError::SignatureError("Signature must be 64 bytes".into()))?;
        let signature = Signature::from_bytes(&sig_arr);

        let digest = self.notice.canonical_digest();
        verifying_key.verify(&digest, &signature).map_err(|e| {
            PolicyError::SignatureError(format!("Revocation signature verification failed: {}", e))
        })
    }
}

/// Regional policy cache maintaining the latest verified policy, durable version floor,
/// and trusted key lifecycle with revocation tracking (W11 / W12 / §3.3.3).
#[derive(Clone, Debug)]
pub struct PolicyCache {
    current: Option<SignedPolicy>,
    highest_seen_version: u64,
    policy_version_floor: u64,
    trusted_issuer_pubkeys: HashSet<String>,
    trusted_authorizer_pubkeys: HashSet<String>,
    revoked_keys: HashSet<String>,
}

impl PolicyCache {
    pub fn new(trusted_issuer_pubkeys: HashSet<String>) -> Self {
        Self {
            current: None,
            highest_seen_version: 0,
            policy_version_floor: 0,
            trusted_authorizer_pubkeys: trusted_issuer_pubkeys.clone(),
            trusted_issuer_pubkeys,
            revoked_keys: HashSet::new(),
        }
    }

    pub fn with_authorizers(mut self, authorizers: HashSet<String>) -> Self {
        self.trusted_authorizer_pubkeys = authorizers;
        self
    }

    pub fn with_version_floor(mut self, floor: u64) -> Self {
        self.policy_version_floor = floor;
        if self.highest_seen_version < floor {
            self.highest_seen_version = floor;
        }
        self
    }

    pub fn set_version_floor(&mut self, floor: u64) {
        if floor > self.policy_version_floor {
            self.policy_version_floor = floor;
        }
        if self.highest_seen_version < floor {
            self.highest_seen_version = floor;
        }
        if let Some(ref cur) = self.current
            && cur.policy.version < floor
        {
            self.current = None;
        }
    }

    pub fn version_floor(&self) -> u64 {
        self.policy_version_floor
    }

    pub fn is_key_revoked(&self, pubkey_hex: &str) -> bool {
        self.revoked_keys.contains(pubkey_hex)
    }

    pub fn revoked_keys(&self) -> &HashSet<String> {
        &self.revoked_keys
    }

    pub fn trusted_issuer_pubkeys(&self) -> &HashSet<String> {
        &self.trusted_issuer_pubkeys
    }

    pub fn add_trusted_key(&mut self, pubkey_hex: String) {
        self.revoked_keys.remove(&pubkey_hex);
        self.trusted_issuer_pubkeys.insert(pubkey_hex);
    }

    /// Process a signed key revocation notice (W12).
    pub fn revoke_key(
        &mut self,
        revocation: SignedKeyRevocation,
        _now_sec: i64,
    ) -> Result<(), PolicyError> {
        revocation.verify_signature()?;

        if self
            .revoked_keys
            .contains(&revocation.notice.authorizer_pubkey_hex)
        {
            return Err(PolicyError::KeyRevoked(format!(
                "Revocation authorizer {} is itself revoked",
                revocation.notice.authorizer_pubkey_hex
            )));
        }

        if !self
            .trusted_authorizer_pubkeys
            .contains(&revocation.notice.authorizer_pubkey_hex)
        {
            return Err(PolicyError::UntrustedAuthorizer(
                revocation.notice.authorizer_pubkey_hex.clone(),
            ));
        }

        let key_to_revoke = revocation.notice.revoked_key_hex.clone();
        self.trusted_issuer_pubkeys.remove(&key_to_revoke);
        self.revoked_keys.insert(key_to_revoke.clone());

        // Invalidate active policy if signed by the newly revoked key
        if let Some(ref cur) = self.current
            && cur.policy.issuer_pubkey_hex == key_to_revoke
        {
            self.current = None;
        }
        Ok(())
    }

    /// Key rotation helper: installs new key and optionally revokes the retired key (W12).
    pub fn rotate_key(
        &mut self,
        new_key_hex: String,
        old_revocation: Option<SignedKeyRevocation>,
        now_sec: i64,
    ) -> Result<(), PolicyError> {
        self.add_trusted_key(new_key_hex);
        if let Some(revocation) = old_revocation {
            self.revoke_key(revocation, now_sec)?;
        }
        Ok(())
    }

    /// Updates cache with a new policy, enforcing INV-9 (signature valid, issuer trusted, version monotonic, floor honored).
    pub fn update(&mut self, signed: SignedPolicy, now_sec: i64) -> Result<(), PolicyError> {
        // 0. Verify issuer is not revoked (W12)
        if self.revoked_keys.contains(&signed.policy.issuer_pubkey_hex) {
            return Err(PolicyError::KeyRevoked(
                signed.policy.issuer_pubkey_hex.clone(),
            ));
        }

        // 1. Verify issuer is trusted
        if !self
            .trusted_issuer_pubkeys
            .contains(&signed.policy.issuer_pubkey_hex)
        {
            return Err(PolicyError::SignatureError(
                "Untrusted policy issuer".into(),
            ));
        }

        // 2. Cryptographic signature check (INV-9)
        signed.verify_signature()?;

        if let Some(current) = &self.current {
            if current.policy.policy_id != signed.policy.policy_id {
                return Err(PolicyError::IdentityChanged);
            }
            if current.policy.version == signed.policy.version
                && current.policy.canonical_digest() != signed.policy.canonical_digest()
            {
                return Err(PolicyError::VersionConflict(signed.policy.version));
            }
        }

        // 3. Durable policy version floor check (W11)
        if signed.policy.version < self.policy_version_floor {
            return Err(PolicyError::VersionBelowFloor {
                floor: self.policy_version_floor,
                received: signed.policy.version,
            });
        }

        // 4. Version rollback prevention (INV-9, §3.3.3)
        if signed.policy.version < self.highest_seen_version {
            return Err(PolicyError::VersionRollback {
                cached: self.highest_seen_version,
                received: signed.policy.version,
            });
        }

        // 5. Freshness check
        if !signed.is_fresh(now_sec) {
            return Err(PolicyError::PolicyStale {
                now_sec,
                expiry_sec: signed.expires_at().unwrap_or(signed.policy.issued_at_sec),
            });
        }

        self.highest_seen_version = signed.policy.version;
        self.current = Some(signed);
        Ok(())
    }

    pub fn current(&self) -> Option<&SignedPolicy> {
        self.current.as_ref()
    }

    pub fn highest_seen_version(&self) -> u64 {
        self.highest_seen_version
    }
}

/// Load trusted public keys from a JSON or line-separated file (W11).
pub fn load_trusted_keys_from_file<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<HashSet<String>, PolicyError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| PolicyError::Io(format!("Failed to read trusted keys file: {}", e)))?;
    let content_trimmed = content.trim();
    if content_trimmed.starts_with('[') {
        let keys: Vec<String> = serde_json::from_str(content_trimmed)?;
        Ok(keys.into_iter().collect())
    } else {
        let keys: HashSet<String> = content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        Ok(keys)
    }
}

/// Load and cryptographically verify policy from a file against trusted keys and durable version floor (W11).
pub fn load_verified_policy_from_file<P: AsRef<std::path::Path>>(
    policy_path: P,
    trusted_keys: &HashSet<String>,
    version_floor: u64,
    now_sec: i64,
) -> Result<SignedPolicy, PolicyError> {
    let content = std::fs::read_to_string(policy_path.as_ref())
        .map_err(|e| PolicyError::Io(format!("Failed to read policy file: {}", e)))?;
    let signed: SignedPolicy = serde_json::from_str(&content)?;

    if !trusted_keys.contains(&signed.policy.issuer_pubkey_hex) {
        return Err(PolicyError::SignatureError(
            "Untrusted policy issuer".into(),
        ));
    }

    signed.verify_signature()?;

    if signed.policy.version < version_floor {
        return Err(PolicyError::VersionBelowFloor {
            floor: version_floor,
            received: signed.policy.version,
        });
    }

    if !signed.is_fresh(now_sec) {
        return Err(PolicyError::PolicyStale {
            now_sec,
            expiry_sec: signed.expires_at().unwrap_or(signed.policy.issued_at_sec),
        });
    }

    Ok(signed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    fn make_test_keys() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    #[test]
    fn test_policy_signing_and_verification() {
        let (signing_key, verifying_key) = make_test_keys();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert("region-eu-1".to_string());
        legal.insert("region-eu-2".to_string());

        let mut modes = HashMap::new();
        modes.insert("region-eu-1".to_string(), DestinationMode::Cleartext);
        modes.insert("region-eu-2".to_string(), DestinationMode::SealedOnly);

        let class_rule = ClassRule {
            class_id: "EU-restricted".to_string(),
            description: "Strict EU sovereign customer data".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-eu-1".to_string(), "region-eu-2".to_string()],
            transit_allowed: false,
        };

        let mut classes = HashMap::new();
        classes.insert("EU-restricted".to_string(), class_rule);

        let policy_doc = PolicyDocument {
            policy_id: "pol-2026-10-01".to_string(),
            version: 1,
            issued_at_sec: 1000,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes,
            transit_rules: HashMap::new(),
        };

        let signed = SignedPolicy::sign(policy_doc, &signing_key);
        assert!(signed.verify_signature().is_ok());
        assert!(signed.is_fresh(1500));
        assert!(!signed.is_fresh(5000)); // expired
    }

    #[test]
    fn test_policy_rollback_rejection() {
        let (signing_key, verifying_key) = make_test_keys();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut trusted = HashSet::new();
        trusted.insert(pubkey_hex.clone());
        let mut cache = PolicyCache::new(trusted);

        let doc_v2 = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 2,
            issued_at_sec: 1000,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed_v2 = SignedPolicy::sign(doc_v2, &signing_key);
        cache.update(signed_v2, 1100).unwrap();

        // Attempt rollback to v1 (must fail)
        let doc_v1 = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 1,
            issued_at_sec: 1000,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed_v1 = SignedPolicy::sign(doc_v1, &signing_key);
        let err = cache.update(signed_v1, 1100).unwrap_err();
        assert!(matches!(err, PolicyError::VersionRollback { .. }));
    }

    #[test]
    fn test_durable_policy_version_floor() {
        let (signing_key, verifying_key) = make_test_keys();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut trusted = HashSet::new();
        trusted.insert(pubkey_hex.clone());

        // Cache initialized with a durable floor of 10 (W11)
        let mut cache = PolicyCache::new(trusted).with_version_floor(10);
        assert_eq!(cache.version_floor(), 10);

        // Attempting to install version 5 must fail with VersionBelowFloor
        let doc_v5 = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 5,
            issued_at_sec: 1000,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed_v5 = SignedPolicy::sign(doc_v5, &signing_key);
        let err = cache.update(signed_v5, 1100).unwrap_err();
        assert!(matches!(
            err,
            PolicyError::VersionBelowFloor {
                floor: 10,
                received: 5
            }
        ));

        // Version 10 or higher must succeed
        let doc_v10 = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 10,
            issued_at_sec: 1000,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed_v10 = SignedPolicy::sign(doc_v10, &signing_key);
        assert!(cache.update(signed_v10, 1100).is_ok());

        // Advancing floor evicts lower versions
        cache.set_version_floor(15);
        assert!(cache.current().is_none());
    }

    #[test]
    fn test_key_revocation_and_rotation_protocol() {
        let (auth_sk, auth_vk) = make_test_keys();
        let auth_pubkey_hex = hex::encode(auth_vk.to_bytes());

        let (issuer1_sk, issuer1_vk) = make_test_keys();
        let issuer1_pubkey_hex = hex::encode(issuer1_vk.to_bytes());

        let (issuer2_sk, issuer2_vk) = make_test_keys();
        let issuer2_pubkey_hex = hex::encode(issuer2_vk.to_bytes());

        let mut trusted = HashSet::new();
        trusted.insert(issuer1_pubkey_hex.clone());

        let mut authorizers = HashSet::new();
        authorizers.insert(auth_pubkey_hex.clone());

        let mut cache = PolicyCache::new(trusted).with_authorizers(authorizers);

        // Policy signed by issuer1 installs successfully
        let doc1 = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 1,
            issued_at_sec: 1000,
            ttl_seconds: 3600,
            issuer_pubkey_hex: issuer1_pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed1 = SignedPolicy::sign(doc1, &issuer1_sk);
        cache.update(signed1, 1100).unwrap();
        assert!(cache.current().is_some());

        // Revoke issuer1 key via signed revocation notice (W12)
        let notice = KeyRevocationNotice::new(
            &issuer1_pubkey_hex,
            "Key compromise simulation",
            1200,
            &auth_pubkey_hex,
        );
        let signed_revocation = SignedKeyRevocation::sign(notice, &auth_sk);

        // Key rotation: install issuer2 and revoke issuer1
        cache
            .rotate_key(issuer2_pubkey_hex.clone(), Some(signed_revocation), 1200)
            .unwrap();

        assert!(cache.is_key_revoked(&issuer1_pubkey_hex));
        assert!(cache.current().is_none()); // Evicted active policy signed by compromised key!

        // Attempting to install any policy signed by revoked key fails immediately
        let doc_revoked = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 2,
            issued_at_sec: 1250,
            ttl_seconds: 3600,
            issuer_pubkey_hex: issuer1_pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed_revoked = SignedPolicy::sign(doc_revoked, &issuer1_sk);
        let err = cache.update(signed_revoked, 1300).unwrap_err();
        assert!(matches!(err, PolicyError::KeyRevoked(_)));

        // New key issuer2 can sign and install policy successfully
        let doc2 = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 2,
            issued_at_sec: 1250,
            ttl_seconds: 3600,
            issuer_pubkey_hex: issuer2_pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed2 = SignedPolicy::sign(doc2, &issuer2_sk);
        assert!(cache.update(signed2, 1300).is_ok());
    }

    #[test]
    fn test_load_verified_policy_from_file() {
        let (signing_key, verifying_key) = make_test_keys();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let doc = PolicyDocument {
            policy_id: "pol-boot".to_string(),
            version: 5,
            issued_at_sec: 1000,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes: HashMap::new(),
            transit_rules: HashMap::new(),
        };
        let signed = SignedPolicy::sign(doc, &signing_key);

        let temp_dir = tempfile::tempdir().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        let keys_path = temp_dir.path().join("trusted_keys.json");

        std::fs::write(&policy_path, serde_json::to_string(&signed).unwrap()).unwrap();
        std::fs::write(
            &keys_path,
            serde_json::to_string(&vec![pubkey_hex.clone()]).unwrap(),
        )
        .unwrap();

        let trusted = load_trusted_keys_from_file(&keys_path).unwrap();
        assert!(trusted.contains(&pubkey_hex));

        // Loading with valid floor passes
        let loaded = load_verified_policy_from_file(&policy_path, &trusted, 5, 1100).unwrap();
        assert_eq!(loaded.policy.version, 5);

        // Loading with floor higher than policy fails
        let err = load_verified_policy_from_file(&policy_path, &trusted, 10, 1100).unwrap_err();
        assert!(matches!(
            err,
            PolicyError::VersionBelowFloor {
                floor: 10,
                received: 5
            }
        ));
    }
}
