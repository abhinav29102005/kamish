use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read configuration file {path:?}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to parse TOML configuration: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Failed to serialize TOML configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
}

fn default_region() -> String {
    "region-a".to_string()
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_data_dir() -> String {
    "./data/broker".to_string()
}

fn default_max_frame_size() -> usize {
    4 * 1024 * 1024 // 4 MB
}

fn default_max_payload_size() -> usize {
    1024 * 1024 // 1 MB
}

fn default_max_connections() -> usize {
    1024
}

/// Server-side resource and network boundary limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerLimits {
    #[serde(default = "default_max_frame_size")]
    pub max_frame_size: usize,

    #[serde(default = "default_max_payload_size")]
    pub max_payload_size: usize,

    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            max_frame_size: default_max_frame_size(),
            max_payload_size: default_max_payload_size(),
            max_connections: default_max_connections(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// TLS 1.3 configuration parameters for mutual authentication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_cert_path: String,
    #[serde(default = "default_true")]
    pub require_client_auth: bool,
}

fn default_partition_count() -> u32 {
    1
}

/// Declarative topic metadata configuration for the tenant catalog (W10 / W13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicConfig {
    pub topic: String,
    pub owner_tenant: String,
    #[serde(default = "default_partition_count")]
    pub partitions: u32,
    pub allowed_classes: Vec<String>,
    #[serde(default)]
    pub authorized_producers: Vec<String>,
    #[serde(default)]
    pub authorized_consumers: Vec<String>,
    #[serde(default)]
    pub allowed_consumer_regions: Vec<String>,
}

/// Broker daemon configuration loadable from a TOML file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerConfig {
    #[serde(default = "default_region")]
    pub region: String,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_data_dir")]
    pub data_dir: String,

    #[serde(default)]
    pub peers: Vec<String>,

    #[serde(default)]
    pub limits: ServerLimits,

    #[serde(default)]
    pub metadata_endpoints: Vec<String>,

    #[serde(default)]
    pub tls: Option<TlsConfig>,

    #[serde(default)]
    pub topics: Vec<TopicConfig>,

    #[serde(default)]
    pub policy_path: Option<String>,

    #[serde(default)]
    pub trusted_keys_path: Option<String>,

    #[serde(default)]
    pub policy_version_floor: Option<u64>,

    #[serde(default)]
    pub node_id: Option<String>,

    #[serde(default)]
    pub ack_level: Option<crate::AckLevel>,

    #[serde(default)]
    pub replica_group: Option<kamish_own::ReplicaGroupConfig>,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            region: default_region(),
            host: default_host(),
            port: default_port(),
            data_dir: default_data_dir(),
            peers: Vec::new(),
            limits: ServerLimits::default(),
            metadata_endpoints: Vec::new(),
            tls: None,
            topics: Vec::new(),
            policy_path: None,
            trusted_keys_path: None,
            policy_version_floor: None,
            node_id: None,
            ack_level: None,
            replica_group: None,
        }
    }
}

impl BrokerConfig {
    /// Load broker configuration from a TOML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref).map_err(|e| ConfigError::Io {
            path: path_ref.display().to_string(),
            source: e,
        })?;
        Self::from_toml(&content)
    }

    /// Parse broker configuration from a TOML string.
    pub fn from_toml(content: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(content)?;
        Ok(config)
    }

    /// Serialize broker configuration into a TOML string.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        let s = toml::to_string_pretty(self)?;
        Ok(s)
    }

    /// Construct a TenantCatalog from declarative topic configurations (W10 / W13).
    pub fn build_catalog(&self) -> crate::catalog::TenantCatalog {
        let mut catalog = crate::catalog::TenantCatalog::new();
        for t in &self.topics {
            let meta = crate::catalog::TopicMetadata::new(
                &t.topic,
                &t.owner_tenant,
                t.partitions,
                t.allowed_classes.iter().cloned().collect(),
            )
            .with_producers(t.authorized_producers.iter().cloned().collect())
            .with_consumers(t.authorized_consumers.iter().cloned().collect())
            .with_consumer_regions(t.allowed_consumer_regions.iter().cloned().collect());
            catalog.register_topic(meta);
        }
        catalog
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_roundtrip() {
        let cfg = BrokerConfig::default();
        let toml_str = cfg.to_toml().unwrap();
        let parsed = BrokerConfig::from_toml(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
        assert_eq!(parsed.limits.max_frame_size, 4 * 1024 * 1024);
        assert_eq!(parsed.limits.max_payload_size, 1024 * 1024);
        assert_eq!(parsed.limits.max_connections, 1024);
    }

    #[test]
    fn test_custom_toml_parsing() {
        let sample = r#"
region = "EU_WEST"
host = "0.0.0.0"
port = 9092
data_dir = "/var/lib/kamish/broker"
peers = ["broker-us:9092", "broker-ap:9092"]
metadata_endpoints = ["http://etcd-1:2379", "http://etcd-2:2379"]

[limits]
max_frame_size = 8388608
max_payload_size = 2097152
max_connections = 2048
"#;
        let parsed = BrokerConfig::from_toml(sample).unwrap();
        assert_eq!(parsed.region, "EU_WEST");
        assert_eq!(parsed.host, "0.0.0.0");
        assert_eq!(parsed.port, 9092);
        assert_eq!(parsed.data_dir, "/var/lib/kamish/broker");
        assert_eq!(parsed.peers, vec!["broker-us:9092", "broker-ap:9092"]);
        assert_eq!(
            parsed.metadata_endpoints,
            vec!["http://etcd-1:2379", "http://etcd-2:2379"]
        );
        assert_eq!(parsed.limits.max_frame_size, 8 * 1024 * 1024);
        assert_eq!(parsed.limits.max_payload_size, 2 * 1024 * 1024);
        assert_eq!(parsed.limits.max_connections, 2048);
    }

    #[test]
    fn test_invalid_toml_fails_closed() {
        let invalid = r#"port = "not_a_number""#;
        let res = BrokerConfig::from_toml(invalid);
        assert!(res.is_err());
    }

    #[test]
    fn test_load_from_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("kamish.toml");
        let content = r#"
region = "AP_SOUTH"
port = 8443
"#;
        std::fs::write(&file_path, content).unwrap();

        let loaded = BrokerConfig::from_file(&file_path).unwrap();
        assert_eq!(loaded.region, "AP_SOUTH");
        assert_eq!(loaded.port, 8443);
        assert_eq!(loaded.limits.max_connections, 1024);
    }
}
