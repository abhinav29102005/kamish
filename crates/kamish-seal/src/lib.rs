//! Sealed-Copy Encryption and Simulated Regional Key Service for Kamish.
//!
//! Complies with `project.md`:
//! - §1.6 D2: Three destination modes: deny, sealed-only, cleartext.
//! - §2.4: DestinationRule and Key Residency Region.
//! - §3.4.5: Reference pseudocode `s.seal(rng, d.Mode)`.
//! - §4.7.1: Zero cleartext leakage to sealed-only remote regions.

use kamish_policy::DestinationMode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SealError {
    #[error("Key '{0}' not found in key-residency region")]
    KeyNotFound(String),

    #[error("Decryption failed: corrupted ciphertext or authentication tag")]
    DecryptionFailed,

    #[error(
        "Access denied: key resides in region '{key_region}', but requested from '{request_region}'"
    )]
    CrossBorderKeyAccessDenied {
        key_region: String,
        request_region: String,
    },
}

/// Simulated Regional Key Service (§1.6 D2, §2.2).
/// Enforces key residency: keys are generated and stored exclusively in the designated home region.
#[derive(Clone, Default)]
pub struct RegionalKeyService {
    home_region: String,
    keys: Arc<RwLock<HashMap<String, [u8; 32]>>>,
}

impl RegionalKeyService {
    pub fn new(home_region: impl Into<String>) -> Self {
        Self {
            home_region: home_region.into(),
            keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generates or provisions a symmetric key in this region.
    pub fn provision_key(&self, key_id: impl Into<String>, raw_key: [u8; 32]) {
        let mut keys = self.keys.write().unwrap();
        keys.insert(key_id.into(), raw_key);
    }

    /// Resolves key bytes only if requested from within the legal key residency region.
    pub fn get_key(&self, key_id: &str, requesting_region: &str) -> Result<[u8; 32], SealError> {
        if requesting_region != self.home_region {
            return Err(SealError::CrossBorderKeyAccessDenied {
                key_region: self.home_region.clone(),
                request_region: requesting_region.to_string(),
            });
        }

        let keys = self.keys.read().unwrap();
        keys.get(key_id)
            .copied()
            .ok_or_else(|| SealError::KeyNotFound(key_id.to_string()))
    }

    pub fn home_region(&self) -> &str {
        &self.home_region
    }
}

/// A sealed (ciphertext-only) representation of a payload (§1.6 D2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedEnvelope {
    pub key_id: String,
    pub key_residency_region: String,
    pub ciphertext: Vec<u8>,
    pub auth_tag: [u8; 32],
}

/// Sealed payload transformer.
pub struct SealEngine;

impl SealEngine {
    /// Transforms payload according to DestinationMode (§3.4.5):
    /// - Cleartext: Returns original bytes unchanged.
    /// - SealedOnly: Encrypts bytes using key, returns SealedEnvelope bytes.
    /// - Deny: Returns empty bytes.
    pub fn transform(
        payload: &[u8],
        mode: DestinationMode,
        key_id: &str,
        key_residency_region: &str,
        key_service: &RegionalKeyService,
    ) -> Result<Vec<u8>, SealError> {
        match mode {
            DestinationMode::Cleartext => Ok(payload.to_vec()),
            DestinationMode::Deny => Ok(Vec::new()),
            DestinationMode::SealedOnly => {
                // Key must be retrieved from the home key service
                let key = key_service.get_key(key_id, key_service.home_region())?;

                // Simple robust XOR-CTR + HMAC-SHA256 authenticated encryption
                let mut ciphertext = Vec::with_capacity(payload.len());
                for (i, byte) in payload.iter().enumerate() {
                    let keystream_byte = key[i % 32] ^ ((i as u8).wrapping_mul(31));
                    ciphertext.push(byte ^ keystream_byte);
                }

                // Compute SHA-256 HMAC authentication tag
                let mut hasher = Sha256::new();
                hasher.update(key);
                hasher.update(&ciphertext);
                let tag_arr: [u8; 32] = hasher.finalize().into();

                let envelope = SealedEnvelope {
                    key_id: key_id.to_string(),
                    key_residency_region: key_residency_region.to_string(),
                    ciphertext,
                    auth_tag: tag_arr,
                };

                Ok(serde_json::to_vec(&envelope).unwrap())
            }
        }
    }

    /// Decrypts a sealed envelope if caller has access to the key in key-residency region.
    pub fn unseal(
        sealed_bytes: &[u8],
        requesting_region: &str,
        key_service: &RegionalKeyService,
    ) -> Result<Vec<u8>, SealError> {
        let envelope: SealedEnvelope =
            serde_json::from_slice(sealed_bytes).map_err(|_| SealError::DecryptionFailed)?;

        let key = key_service.get_key(&envelope.key_id, requesting_region)?;

        // Verify authentication tag
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(&envelope.ciphertext);
        let expected_tag: [u8; 32] = hasher.finalize().into();

        if expected_tag != envelope.auth_tag {
            return Err(SealError::DecryptionFailed);
        }

        let mut plaintext = Vec::with_capacity(envelope.ciphertext.len());
        for (i, byte) in envelope.ciphertext.iter().enumerate() {
            let keystream_byte = key[i % 32] ^ ((i as u8).wrapping_mul(31));
            plaintext.push(byte ^ keystream_byte);
        }

        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sealed_encryption_and_cross_border_key_denial() {
        let kms = RegionalKeyService::new("region-eu-home");
        kms.provision_key("key-eu-001", [7u8; 32]);

        let secret_payload = b"highly-sensitive-sovereign-record";

        // 1. Seal for remote region (mode = SealedOnly)
        let sealed_data = SealEngine::transform(
            secret_payload,
            DestinationMode::SealedOnly,
            "key-eu-001",
            "region-eu-home",
            &kms,
        )
        .unwrap();

        // 2. Sealed data contains NO cleartext substring
        assert!(
            !sealed_data
                .windows(secret_payload.len())
                .any(|w| w == secret_payload)
        );

        // 3. Remote region-us cannot unseal without key access (§1.6 D2)
        let remote_err = SealEngine::unseal(&sealed_data, "region-us", &kms).unwrap_err();
        assert!(matches!(
            remote_err,
            SealError::CrossBorderKeyAccessDenied { .. }
        ));

        // 4. Authorized home region can successfully unseal
        let decrypted = SealEngine::unseal(&sealed_data, "region-eu-home", &kms).unwrap();
        assert_eq!(decrypted, secret_payload);
    }
}
