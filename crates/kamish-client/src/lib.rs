//! High-Level Ergonomic Client Library for Kamish Distributed Event Broker.
//!
//! Complies with Developer B Specification:
//! - High-level idempotent producer with automatic sequence numbering (§3.4.2, INV-6)
//! - Consumer with offset tracking bounded by High-Water Mark (INV-4)
//! - Client-side residency validation hooks with Developer B's `PolicyEngine`
//! - Real-time replication lag monitoring and administration

use kamish_broker::{BrokerClient, BrokerRequest, BrokerResponse, FetchRequest, ProduceRequest, ProduceResponse};
use kamish_log::RecordEnvelope;
use kamish_meta::{PolicyEngine, RegionId, ResidencyPolicy, StreamEvent};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::net::ToSocketAddrs;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Network / IO error: {0}")]
    Network(String),

    #[error("Broker rejected operation: {0}")]
    BrokerRejection(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Unexpected response from broker")]
    UnexpectedResponse,
}

/// Thread-safe Idempotent Producer Client.
pub struct KamishProducer {
    client: Arc<tokio::sync::Mutex<BrokerClient>>,
    producer_id: u64,
    producer_epoch: u64,
    sequence_seq: AtomicU32,
    local_region: RegionId,
    policy_engine: Option<Arc<dyn PolicyEngine>>,
}

impl KamishProducer {
    pub async fn connect<A: ToSocketAddrs>(
        addr: A,
        producer_id: u64,
        local_region: RegionId,
        policy_engine: Option<Arc<dyn PolicyEngine>>,
    ) -> Result<Self, ClientError> {
        let client = BrokerClient::connect(addr)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        Ok(Self {
            client: Arc::new(tokio::sync::Mutex::new(client)),
            producer_id,
            producer_epoch: 1,
            sequence_seq: AtomicU32::new(0),
            local_region,
            policy_engine,
        })
    }

    /// Publishes a record with automatic sequence numbering and pre-flight residency check.
    pub async fn send(
        &self,
        topic: &str,
        partition: u32,
        class_id: &str,
        payload: Vec<u8>,
        residency_rule: ResidencyPolicy,
    ) -> Result<ProduceResponse, ClientError> {
        // Optional client-side pre-flight policy verification (§3.B)
        if let Some(ref engine) = self.policy_engine {
            let event = StreamEvent::new(
                0,
                topic,
                format!("{}-{}", topic, partition),
                self.local_region.clone(),
                residency_rule,
                payload.clone(),
            );
            if let Err(violation) = engine.validate_ingestion(&event, &self.local_region) {
                return Err(ClientError::PolicyViolation(violation.to_string()));
            }
        }

        let seq = self.sequence_seq.fetch_add(1, Ordering::SeqCst);
        let req = BrokerRequest::Produce(ProduceRequest {
            topic: topic.to_string(),
            partition,
            producer_id: self.producer_id,
            producer_epoch: self.producer_epoch,
            sequence_base: seq,
            class_id: class_id.to_string(),
            payload,
        });

        let mut client = self.client.lock().await;
        let resp = client
            .send_request(req)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        match resp {
            BrokerResponse::Produce(ack) => {
                if !ack.sovereignty_allowed {
                    return Err(ClientError::BrokerRejection(ack.denial_reason));
                }
                Ok(ack)
            }
            BrokerResponse::Error(err) => Err(ClientError::BrokerRejection(err)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }
}

/// Sequential Consumer Client bounded by High-Water Mark (INV-4).
pub struct KamishConsumer {
    client: Arc<tokio::sync::Mutex<BrokerClient>>,
    current_offset: u64,
}

impl KamishConsumer {
    pub async fn connect<A: ToSocketAddrs>(addr: A, start_offset: u64) -> Result<Self, ClientError> {
        let client = BrokerClient::connect(addr)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        Ok(Self {
            client: Arc::new(tokio::sync::Mutex::new(client)),
            current_offset: start_offset,
        })
    }

    /// Fetches next available batch of records.
    pub async fn fetch(
        &mut self,
        topic: &str,
        partition: u32,
        class_id: &str,
        max_records: usize,
    ) -> Result<Vec<RecordEnvelope>, ClientError> {
        let req = BrokerRequest::Fetch(FetchRequest {
            topic: topic.to_string(),
            partition,
            class_id: class_id.to_string(),
            start_offset: self.current_offset,
            max_records,
        });

        let mut client = self.client.lock().await;
        let resp = client
            .send_request(req)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        match resp {
            BrokerResponse::Fetch(batch) => {
                if let Some(last) = batch.records.last() {
                    self.current_offset = last.base_offset + 1;
                }
                Ok(batch.records)
            }
            BrokerResponse::Error(err) => Err(ClientError::BrokerRejection(err)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    pub fn current_offset(&self) -> u64 {
        self.current_offset
    }
}

/// Administrative Client for metrics and watermark queries.
pub struct KamishAdminClient {
    client: Arc<tokio::sync::Mutex<BrokerClient>>,
}

impl KamishAdminClient {
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self, ClientError> {
        let client = BrokerClient::connect(addr)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        Ok(Self {
            client: Arc::new(tokio::sync::Mutex::new(client)),
        })
    }

    /// Queries the remote durable watermark for a partition.
    pub async fn get_watermark(&self, topic: &str, partition: u32, source_region: &str) -> Result<u64, ClientError> {
        let req = BrokerRequest::GetWatermark {
            topic: topic.to_string(),
            partition,
            source_region: source_region.to_string(),
        };

        let mut client = self.client.lock().await;
        let resp = client
            .send_request(req)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        match resp {
            BrokerResponse::Watermark { durable_watermark } => Ok(durable_watermark),
            BrokerResponse::Error(err) => Err(ClientError::BrokerRejection(err)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kamish_audit::AuditLog;
    use kamish_broker::BrokerServer;
    use kamish_enforce::Enforcer;
    use kamish_meta::MetadataStore;
    use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy};
    use rand::rngs::OsRng;
    use ed25519_dalek::SigningKey;
    use std::collections::{HashMap, HashSet};
    use std::sync::RwLock;

    struct TestBroker {
        _temp_dir: tempfile::TempDir,
        addr: String,
    }

    async fn start_test_broker(region: &str) -> TestBroker {
        let temp_dir = tempfile::tempdir().unwrap();
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert(region.to_string());

        let mut modes = HashMap::new();
        modes.insert(region.to_string(), DestinationMode::Cleartext);

        let class_rule = ClassRule {
            class_id: "general".to_string(),
            description: "General topic".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec![region.to_string()],
            transit_allowed: false,
        };

        let mut classes = HashMap::new();
        classes.insert("general".to_string(), class_rule);

        let policy_doc = PolicyDocument {
            policy_id: "pol-test".to_string(),
            version: 1,
            issued_at_sec: chrono::Utc::now().timestamp() - 5,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes,
            transit_rules: HashMap::new(),
        };

        let signed = SignedPolicy::sign(policy_doc, &signing_key);
        let mut trusted = HashSet::new();
        trusted.insert(pubkey_hex);
        let mut cache = PolicyCache::new(trusted);
        cache.update(signed, chrono::Utc::now().timestamp()).unwrap();

        let audit = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(region, Arc::new(RwLock::new(cache)), audit));
        let meta = Arc::new(MetadataStore::new());

        let server = Arc::new(BrokerServer::new(region, enforcer, meta, Vec::new(), temp_dir.path()));
        let (local_addr, _handle) = server.start_tcp_listener("127.0.0.1:0").await.unwrap();
        let addr = local_addr.to_string();

        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        TestBroker {
            _temp_dir: temp_dir,
            addr,
        }
    }

    #[tokio::test]
    async fn test_client_produce_and_consume_workflow() {
        let broker = start_test_broker("region-client-test").await;

        let producer = KamishProducer::connect(
            &broker.addr,
            888,
            RegionId::Custom("region-client-test".to_string()),
            None,
        )
        .await
        .unwrap();

        let ack = producer
            .send("telemetry", 0, "general", b"sample-reading".to_vec(), ResidencyPolicy::GlobalCleartext)
            .await
            .unwrap();
        assert_eq!(ack.base_offset, 0);

        let mut consumer = KamishConsumer::connect(&broker.addr, 0).await.unwrap();
        let records = consumer.fetch("telemetry", 0, "general", 10).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].payload, b"sample-reading");
        assert_eq!(consumer.current_offset(), 1);
    }
}
