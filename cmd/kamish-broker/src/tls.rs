use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::sync::Arc;
use thiserror::Error;
use x509_parser::prelude::*;

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Rustls error: {0}")]
    Rustls(#[from] rustls::Error),

    #[error("Certificate parse error: {0}")]
    CertificateParse(String),

    #[error("Private key parse error: {0}")]
    PrivateKeyParse(String),

    #[error("Client presented no certificate during mTLS handshake")]
    NoCertificatePresented,

    #[error("Peer identity extraction failed: {0}")]
    IdentityExtraction(String),

    #[error("Authorization denied: principal {principal:?} lacks required role {required:?}")]
    RoleDenied { principal: String, required: String },

    #[error(
        "Tenant authorization mismatch: client tenant {client_tenant:?} cannot access resource tenant {resource_tenant:?}"
    )]
    TenantMismatch {
        client_tenant: String,
        resource_tenant: String,
    },
}

/// Authenticated identity of a peer derived directly from a verified X.509 certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientIdentity {
    pub principal: String,
    pub tenant: String,
    pub roles: Vec<String>,
    pub region: String,
}

impl ClientIdentity {
    pub fn new(
        principal: impl Into<String>,
        tenant: impl Into<String>,
        roles: Vec<String>,
        region: impl Into<String>,
    ) -> Self {
        Self {
            principal: principal.into(),
            tenant: tenant.into(),
            roles,
            region: region.into(),
        }
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles
            .iter()
            .any(|r| r.eq_ignore_ascii_case(role) || r.eq_ignore_ascii_case("Admin"))
    }

    pub fn is_authorized_for_tenant(&self, target_tenant: &str) -> bool {
        self.has_role("Admin") || self.tenant == target_tenant || self.tenant == "*"
    }
}

/// Parses an X.509 certificate DER and extracts strongly-typed `ClientIdentity`.
/// Supports structured CommonName format: "principal:tenant:role1,role2:region"
/// or standard X.509 fields (CN = principal, O = tenant, OU = roles, ST/L = region).
pub fn extract_client_identity(cert_der: &[u8]) -> Result<ClientIdentity, TlsError> {
    let (_, x509) = parse_x509_certificate(cert_der).map_err(|e| {
        TlsError::CertificateParse(format!("Failed to parse DER certificate: {}", e))
    })?;

    let subject = x509.subject();

    // 1. Extract Common Name (CN)
    let cn = subject
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .ok_or_else(|| {
            TlsError::IdentityExtraction("Missing Common Name in client certificate".into())
        })?;

    // Check if CN is encoded as structured string: "principal:tenant:role1,role2:region"
    let parts: Vec<&str> = cn.split(':').collect();
    if parts.len() == 4 {
        let principal = parts[0].to_string();
        let tenant = parts[1].to_string();
        let roles: Vec<String> = parts[2]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let region = parts[3].to_string();
        return Ok(ClientIdentity {
            principal,
            tenant,
            roles,
            region,
        });
    }

    // 2. Standard X.509 fields:
    let principal = cn.to_string();

    let tenant = subject
        .iter_organization()
        .next()
        .and_then(|o| o.as_str().ok())
        .unwrap_or(&principal)
        .to_string();

    let mut roles = Vec::new();
    for ou in subject.iter_organizational_unit() {
        if let Ok(ou_str) = ou.as_str() {
            for r in ou_str.split(',') {
                let trimmed = r.trim();
                if !trimmed.is_empty() {
                    roles.push(trimmed.to_string());
                }
            }
        }
    }
    if roles.is_empty() {
        roles.push("Client".to_string());
    }

    let region = subject
        .iter_locality()
        .next()
        .and_then(|l| l.as_str().ok())
        .unwrap_or("GLOBAL")
        .to_string();

    Ok(ClientIdentity {
        principal,
        tenant,
        roles,
        region,
    })
}

/// Builds a TLS 1.3 `ServerConfig` enforcing mutual authentication (mTLS) via pure-Rust `rustls`.
pub fn build_server_config(
    cert_pem: &[u8],
    key_pem: &[u8],
    ca_cert_pem: Option<&[u8]>,
    require_client_auth: bool,
) -> Result<Arc<ServerConfig>, TlsError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // 1. Parse server certificate chain
    let mut cert_cursor = Cursor::new(cert_pem);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_cursor)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            TlsError::CertificateParse(format!("Failed to parse server cert PEM: {}", e))
        })?;
    if certs.is_empty() {
        return Err(TlsError::CertificateParse(
            "No certificates found in cert_pem".into(),
        ));
    }

    // 2. Parse server private key
    let mut key_cursor = Cursor::new(key_pem);
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_cursor)
        .map_err(|e| TlsError::PrivateKeyParse(format!("Failed to read private key PEM: {}", e)))?
        .ok_or_else(|| TlsError::PrivateKeyParse("No private key found in key_pem".into()))?;

    // 3. Configure TLS 1.3 protocol version only
    let builder =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])?;

    // 4. Configure mutual authentication (client certificate verification)
    let config = if require_client_auth {
        let ca_pem = ca_cert_pem.ok_or_else(|| {
            TlsError::CertificateParse("ca_cert_pem required for mutual authentication".into())
        })?;
        let mut ca_cursor = Cursor::new(ca_pem);
        let mut root_store = RootCertStore::empty();
        for cert in rustls_pemfile::certs(&mut ca_cursor) {
            let c = cert.map_err(|e| {
                TlsError::CertificateParse(format!("Failed to parse CA cert: {}", e))
            })?;
            root_store.add(c)?;
        }
        let client_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|e| TlsError::CertificateParse(e.to_string()))?;
        builder
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certs, key)?
    } else {
        builder.with_no_client_auth().with_single_cert(certs, key)?
    };

    Ok(Arc::new(config))
}

/// Builds a TLS 1.3 `ClientConfig` with optional client certificate for mutual authentication.
pub fn build_client_config(
    ca_cert_pem: &[u8],
    client_cert_pem: Option<&[u8]>,
    client_key_pem: Option<&[u8]>,
) -> Result<Arc<ClientConfig>, TlsError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // 1. Root certificate store for verifying the server
    let mut ca_cursor = Cursor::new(ca_cert_pem);
    let mut root_store = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut ca_cursor) {
        let c = cert
            .map_err(|e| TlsError::CertificateParse(format!("Failed to parse CA cert: {}", e)))?;
        root_store.add(c)?;
    }

    let builder =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_root_certificates(root_store);

    // 2. Client certificate for mTLS if provided
    let config = if let (Some(cert_pem), Some(key_pem)) = (client_cert_pem, client_key_pem) {
        let mut cert_cursor = Cursor::new(cert_pem);
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_cursor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                TlsError::CertificateParse(format!("Failed to parse client cert PEM: {}", e))
            })?;

        let mut key_cursor = Cursor::new(key_pem);
        let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_cursor)
            .map_err(|e| {
                TlsError::PrivateKeyParse(format!("Failed to parse client private key: {}", e))
            })?
            .ok_or_else(|| {
                TlsError::PrivateKeyParse("No private key found in client_key_pem".into())
            })?;

        builder.with_client_auth_cert(certs, key)?
    } else {
        builder.with_no_client_auth()
    };

    Ok(Arc::new(config))
}

pub mod test_helpers {
    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair,
        KeyUsagePurpose,
    };

    pub struct GeneratedCa {
        pub cert_pem: String,
        pub cert_der: Vec<u8>,
        pub params: CertificateParams,
        pub key_pair: KeyPair,
    }

    pub struct GeneratedCert {
        pub cert_pem: String,
        pub cert_der: Vec<u8>,
        pub key_pem: String,
    }

    pub fn make_ca(cn: &str) -> GeneratedCa {
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, cn);
        dn.push(DnType::OrganizationName, "Kamish Root Authority");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        GeneratedCa {
            cert_pem: cert.pem(),
            cert_der: cert.der().to_vec(),
            params,
            key_pair,
        }
    }

    pub fn make_cert_signed_by_ca(
        ca: &GeneratedCa,
        cn: &str,
        org: &str,
        ou_roles: &[&str],
        locality_region: &str,
    ) -> GeneratedCert {
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, cn);
        dn.push(DnType::OrganizationName, org);
        if !ou_roles.is_empty() {
            dn.push(DnType::OrganizationalUnitName, ou_roles.join(","));
        }
        dn.push(DnType::LocalityName, locality_region);
        params.distinguished_name = dn;
        params.subject_alt_names = vec![rcgen::SanType::DnsName(
            "localhost".to_string().try_into().unwrap(),
        )];

        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let key_pem = key_pair.serialize_pem();

        let issuer = rcgen::Issuer::from_params(&ca.params, &ca.key_pair);
        let cert = params.signed_by(&key_pair, &issuer).unwrap();

        GeneratedCert {
            cert_pem: cert.pem(),
            cert_der: cert.der().to_vec(),
            key_pem,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_client_identity_roles_and_tenant_check() {
        let id = ClientIdentity::new("producer-1", "tenant-a", vec!["Producer".into()], "EU_WEST");
        assert!(id.has_role("Producer"));
        assert!(!id.has_role("Consumer"));
        assert!(id.is_authorized_for_tenant("tenant-a"));
        assert!(!id.is_authorized_for_tenant("tenant-b"));

        let admin = ClientIdentity::new("admin-user", "ops", vec!["Admin".into()], "EU_WEST");
        assert!(admin.has_role("Admin"));
        assert!(admin.has_role("Producer")); // Admin implies all roles
        assert!(admin.is_authorized_for_tenant("tenant-b")); // Admin accesses any tenant
    }

    #[test]
    fn test_extract_client_identity_structured_cn() {
        let ca = make_ca("Kamish Test CA");
        let cert = make_cert_signed_by_ca(
            &ca,
            "orders-svc:fintech:Producer,Agent:EU_WEST",
            "IgnoredOrg",
            &[],
            "IgnoredLoc",
        );

        let id = extract_client_identity(&cert.cert_der).unwrap();
        assert_eq!(id.principal, "orders-svc");
        assert_eq!(id.tenant, "fintech");
        assert_eq!(id.roles, vec!["Producer", "Agent"]);
        assert_eq!(id.region, "EU_WEST");
        assert!(id.has_role("Producer"));
        assert!(id.has_role("Agent"));
    }

    #[test]
    fn test_extract_client_identity_standard_x509_fields() {
        let ca = make_ca("Kamish Test CA");
        let cert = make_cert_signed_by_ca(
            &ca,
            "reporting-svc",
            "analytics-corp",
            &["Consumer", "Auditor"],
            "US_EAST",
        );

        let id = extract_client_identity(&cert.cert_der).unwrap();
        assert_eq!(id.principal, "reporting-svc");
        assert_eq!(id.tenant, "analytics-corp");
        assert_eq!(id.roles, vec!["Consumer", "Auditor"]);
        assert_eq!(id.region, "US_EAST");
    }

    #[test]
    fn test_build_configs_and_verify_valid_tls13() {
        let ca = make_ca("Kamish Test CA");
        let server_cert =
            make_cert_signed_by_ca(&ca, "localhost", "Kamish Broker", &["Broker"], "EU_WEST");
        let client_cert =
            make_cert_signed_by_ca(&ca, "client-app", "tenant-1", &["Producer"], "EU_WEST");

        let server_cfg = build_server_config(
            server_cert.cert_pem.as_bytes(),
            server_cert.key_pem.as_bytes(),
            Some(ca.cert_pem.as_bytes()),
            true,
        );
        assert!(
            server_cfg.is_ok(),
            "Server TLS 1.3 config with client auth must succeed"
        );

        let client_cfg = build_client_config(
            ca.cert_pem.as_bytes(),
            Some(client_cert.cert_pem.as_bytes()),
            Some(client_cert.key_pem.as_bytes()),
        );
        assert!(
            client_cfg.is_ok(),
            "Client TLS 1.3 config with client auth must succeed"
        );
    }
}
