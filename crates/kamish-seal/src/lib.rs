//! Authenticated Encryption with Associated Data (AEAD) Sealed Replication and Key Custody for Kamish.
//!
//! Complies with `project.md`:
//! - §1.6 D2: Three destination modes: deny, sealed-only, cleartext.
//! - §2.4: DestinationRule and Key Residency Region.
//! - §3.4.5: Reference pseudocode `s.seal(rng, d.Mode)`.
//! - §4.7.1: Zero cleartext leakage to sealed-only remote regions.
//!
//! Production Cryptographic Contract (Phase 6 / Workstream A):
//! - AEAD ciphers: ChaCha20-Poly1305 (IETF RFC 8439) and AES-256-GCM.
//! - 96-bit cryptographically secure nonces generated via OS CSPRNG.
//! - AAD binds topic, partition, epoch, policy_version, key_id, and key_version.
//! - Authentication tag (128-bit) verified in constant time before plaintext is accepted.
//! - Pluggable KeyProvider trait supporting external KMS/HSM boundaries.
//! - Non-production DevKeyProvider explicitly isolated from production paths.

use kamish_policy::DestinationMode;
use rand::RngCore;
use ring::aead::{AES_256_GCM, Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Supported AEAD ciphers for sealed cross-region envelopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AeadAlgorithm {
    #[default]
    ChaCha20Poly1305,
    Aes256Gcm,
}

impl AeadAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ChaCha20Poly1305 => "CHACHA20-POLY1305",
            Self::Aes256Gcm => "AES-256-GCM",
        }
    }
}

#[derive(Debug, Error)]
pub enum SealError {
    #[error("Key '{key_id}' (version {key_version}) not found in key-residency region")]
    KeyNotFound { key_id: String, key_version: u32 },

    #[error("Decryption failed: corrupted ciphertext, corrupted tag, or invalid AAD binding")]
    DecryptionFailed,

    #[error("Unsupported AEAD algorithm: {0}")]
    UnsupportedAlgorithm(String),

    #[error("Invalid key length: expected 32 bytes for 256-bit AEAD key, found {0}")]
    InvalidKeyLength(usize),

    #[error(
        "Access denied: key resides in region '{key_region}', but requested from '{request_region}'"
    )]
    CrossBorderKeyAccessDenied {
        key_region: String,
        request_region: String,
    },

    #[error("Cryptographic error: {0}")]
    CryptoError(String),

    #[error("KMS provider error: {0}")]
    KmsProviderError(String),

    #[error("KMS custody unconfigured: production sealed replication requires a configured KMS")]
    KmsUnconfigured,
}

/// Key Provider Trait for resolving 256-bit symmetric keys with regional residency enforcement.
pub trait KeyProvider: Send + Sync {
    /// Resolves the 256-bit symmetric key for a given key ID and version.
    /// Access must be denied if the requesting region is outside the legal residency boundary.
    fn get_key(
        &self,
        key_id: &str,
        key_version: u32,
        requesting_region: &str,
    ) -> Result<[u8; 32], SealError>;

    /// The geographic or sovereign region where key material is legally held.
    fn home_region(&self) -> &str;

    /// Provider identifier (e.g., "dev-mock", "aws-kms", "vault").
    fn provider_type(&self) -> &'static str;

    /// Whether this provider is safe for production use.
    fn is_production_ready(&self) -> bool;
}

/// NON-PRODUCTION DEVELOPMENT KEY PROVIDER (§1.6 D2, §2.2).
///
/// WARNING: This provider stores raw key bytes in local process memory and is
/// strictly intended for automated tests, development clusters, and scenario validation.
/// It must NEVER be used in a production deployment where hardware key custody is mandated.
type KeyMap = Arc<RwLock<HashMap<(String, u32), [u8; 32]>>>;

#[derive(Clone, Default)]
pub struct DevKeyProvider {
    home_region: String,
    keys: KeyMap,
}

impl DevKeyProvider {
    pub fn new(home_region: impl Into<String>) -> Self {
        Self {
            home_region: home_region.into(),
            keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Provisions a symmetric 256-bit key in volatile memory for local development.
    pub fn provision_key(&self, key_id: impl Into<String>, version: u32, raw_key: [u8; 32]) {
        let mut keys = self.keys.write().unwrap();
        keys.insert((key_id.into(), version), raw_key);
    }
}

impl KeyProvider for DevKeyProvider {
    fn get_key(
        &self,
        key_id: &str,
        key_version: u32,
        requesting_region: &str,
    ) -> Result<[u8; 32], SealError> {
        if requesting_region != self.home_region {
            return Err(SealError::CrossBorderKeyAccessDenied {
                key_region: self.home_region.clone(),
                request_region: requesting_region.to_string(),
            });
        }

        let keys = self.keys.read().unwrap();
        keys.get(&(key_id.to_string(), key_version))
            .copied()
            .ok_or_else(|| SealError::KeyNotFound {
                key_id: key_id.to_string(),
                key_version,
            })
    }

    fn home_region(&self) -> &str {
        &self.home_region
    }

    fn provider_type(&self) -> &'static str {
        "dev-memory-mock"
    }

    fn is_production_ready(&self) -> bool {
        false
    }
}

/// External KMS Key Provider Configuration & Boundary.
///
/// Encapsulates the endpoint, authentication, and key ARN for cloud hardware security modules
/// (e.g. AWS KMS, Google Cloud KMS, HashiCorp Vault). In this release, without live credentials,
/// this provider validates configuration and fails closed safely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalKmsConfig {
    pub provider_name: String,
    pub endpoint_url: String,
    pub key_arn: String,
    pub home_region: String,
    pub key_version: u32,
    pub timeout_ms: u64,
}

pub struct ExternalKmsProvider {
    config: ExternalKmsConfig,
}

impl ExternalKmsProvider {
    pub fn new(config: ExternalKmsConfig) -> Self {
        Self { config }
    }
}

impl KeyProvider for ExternalKmsProvider {
    fn get_key(
        &self,
        key_id: &str,
        key_version: u32,
        requesting_region: &str,
    ) -> Result<[u8; 32], SealError> {
        if requesting_region != self.config.home_region {
            return Err(SealError::CrossBorderKeyAccessDenied {
                key_region: self.config.home_region.clone(),
                request_region: requesting_region.to_string(),
            });
        }

        if key_id != self.config.key_arn && key_id != "default" {
            return Err(SealError::KeyNotFound {
                key_id: key_id.to_string(),
                key_version,
            });
        }

        // Without an active network KMS connection and valid cloud credentials, fail closed safely.
        Err(SealError::KmsProviderError(format!(
            "External KMS provider '{}' at '{}' requires active cloud credentials and TLS mutual auth",
            self.config.provider_name, self.config.endpoint_url
        )))
    }

    fn home_region(&self) -> &str {
        &self.config.home_region
    }

    fn provider_type(&self) -> &'static str {
        "external-kms-boundary"
    }

    fn is_production_ready(&self) -> bool {
        true
    }
}

/// Authenticated AEAD Sealed Envelope (§1.6 D2, §3.4.5).
///
/// Protects payload confidentiality and integrity during cross-region WAN transit.
/// Includes 96-bit nonce, key identifier, key version, algorithm identifier, and AAD parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedEnvelope {
    pub version: u32,
    pub algorithm: AeadAlgorithm,
    pub key_id: String,
    pub key_version: u32,
    pub key_residency_region: String,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub topic: String,
    pub partition: u32,
    pub epoch: u64,
    pub policy_version: u64,
}

/// Builds Associated Authenticated Data (AAD) binding to cryptographic envelope.
pub fn compute_aead_aad(
    topic: &str,
    partition: u32,
    epoch: u64,
    policy_version: u64,
    key_id: &str,
    key_version: u32,
) -> Vec<u8> {
    format!(
        "kamish-aead-v1:{}:{}:{}:{}:{}:{}",
        topic, partition, epoch, policy_version, key_id, key_version
    )
    .into_bytes()
}

/// Authenticated Encryption Engine implementing ChaCha20-Poly1305 and AES-256-GCM.
pub struct AeadSealEngine;

impl AeadSealEngine {
    /// Encrypts and authenticates payload with the requested AEAD algorithm.
    #[allow(clippy::too_many_arguments)]
    pub fn seal(
        payload: &[u8],
        algorithm: AeadAlgorithm,
        topic: &str,
        partition: u32,
        epoch: u64,
        policy_version: u64,
        key_id: &str,
        key_version: u32,
        key_provider: &dyn KeyProvider,
    ) -> Result<SealedEnvelope, SealError> {
        let raw_key = key_provider.get_key(key_id, key_version, key_provider.home_region())?;

        // 1. Generate 96-bit cryptographically secure random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        // 2. Select cipher algorithm
        let ring_alg = match algorithm {
            AeadAlgorithm::ChaCha20Poly1305 => &CHACHA20_POLY1305,
            AeadAlgorithm::Aes256Gcm => &AES_256_GCM,
        };

        // 3. Bind key and initialize LessSafeKey
        let unbound_key = UnboundKey::new(ring_alg, &raw_key)
            .map_err(|e| SealError::CryptoError(format!("UnboundKey error: {e:?}")))?;
        let key = LessSafeKey::new(unbound_key);

        // 4. Construct AAD
        let aad_bytes =
            compute_aead_aad(topic, partition, epoch, policy_version, key_id, key_version);
        let aad = Aad::from(&aad_bytes);

        // 5. Encrypt in-place with tag appended
        let mut in_out = payload.to_vec();
        let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .map_err(|e| SealError::CryptoError(format!("Nonce error: {e:?}")))?;

        key.seal_in_place_append_tag(nonce, aad, &mut in_out)
            .map_err(|e| SealError::CryptoError(format!("Encryption error: {e:?}")))?;

        Ok(SealedEnvelope {
            version: 2,
            algorithm,
            key_id: key_id.to_string(),
            key_version,
            key_residency_region: key_provider.home_region().to_string(),
            nonce: nonce_bytes,
            ciphertext: in_out,
            topic: topic.to_string(),
            partition,
            epoch,
            policy_version,
        })
    }

    /// Verifies authentication tag and decrypts sealed envelope.
    pub fn unseal(
        envelope: &SealedEnvelope,
        requesting_region: &str,
        key_provider: &dyn KeyProvider,
    ) -> Result<Vec<u8>, SealError> {
        let raw_key =
            key_provider.get_key(&envelope.key_id, envelope.key_version, requesting_region)?;

        let ring_alg = match envelope.algorithm {
            AeadAlgorithm::ChaCha20Poly1305 => &CHACHA20_POLY1305,
            AeadAlgorithm::Aes256Gcm => &AES_256_GCM,
        };

        let unbound_key = UnboundKey::new(ring_alg, &raw_key)
            .map_err(|e| SealError::CryptoError(format!("UnboundKey error: {e:?}")))?;
        let key = LessSafeKey::new(unbound_key);

        let aad_bytes = compute_aead_aad(
            &envelope.topic,
            envelope.partition,
            envelope.epoch,
            envelope.policy_version,
            &envelope.key_id,
            envelope.key_version,
        );
        let aad = Aad::from(&aad_bytes);

        let nonce = Nonce::try_assume_unique_for_key(&envelope.nonce)
            .map_err(|e| SealError::CryptoError(format!("Nonce error: {e:?}")))?;

        let mut in_out = envelope.ciphertext.clone();
        let plaintext_slice = key
            .open_in_place(nonce, aad, &mut in_out)
            .map_err(|_| SealError::DecryptionFailed)?;

        Ok(plaintext_slice.to_vec())
    }
}

/// Legacy / convenience transformer matching `project.md` §3.4.5.
pub struct SealEngine;

impl SealEngine {
    /// Transforms payload according to DestinationMode (§3.4.5).
    pub fn transform(
        payload: &[u8],
        mode: DestinationMode,
        key_id: &str,
        _key_residency_region: &str,
        key_provider: &dyn KeyProvider,
    ) -> Result<Vec<u8>, SealError> {
        match mode {
            DestinationMode::Cleartext => Ok(payload.to_vec()),
            DestinationMode::Deny => Ok(Vec::new()),
            DestinationMode::SealedOnly => {
                let envelope = AeadSealEngine::seal(
                    payload,
                    AeadAlgorithm::ChaCha20Poly1305,
                    "default",
                    0,
                    1,
                    1,
                    key_id,
                    1,
                    key_provider,
                )?;
                serde_json::to_vec(&envelope).map_err(|e| SealError::CryptoError(e.to_string()))
            }
        }
    }

    /// Decrypts serialized sealed envelope bytes.
    pub fn unseal(
        sealed_bytes: &[u8],
        requesting_region: &str,
        key_provider: &dyn KeyProvider,
    ) -> Result<Vec<u8>, SealError> {
        let envelope: SealedEnvelope =
            serde_json::from_slice(sealed_bytes).map_err(|_| SealError::DecryptionFailed)?;
        AeadSealEngine::unseal(&envelope, requesting_region, key_provider)
    }
}

// Preserve backwards compatibility type alias for existing codebase references
pub type RegionalKeyService = DevKeyProvider;
