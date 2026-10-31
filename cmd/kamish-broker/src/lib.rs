//! Kamish Regional Broker Daemon and Core Service Engine.
//!
//! Complies with `project.md`:
//! - §2.6: Enforcement Points Map (E1/E2 Ingress, E4 Egress, E5 Inbound Replication, E7 Fetch).
//! - §2.7: Data Flow Diagrams Levels 0 to 3.
//! - §3.1.1: Produce Path with Policy Enforcement and Acknowledgment.
//! - §3.1.2: Cross-Region Replication with Durable Outbox and Resume.
//! - §3.A: Canonical StreamEvent shared model.
//! - §3.B: PolicyEngine Ingress and Egress Interceptor Hooks.
//! - INV-1: Dense monotonic offsets per partition.
//! - INV-4: Fetch bounded by committed high-water mark.
//! - INV-5: Destination log is a prefix of source log.
//! - INV-6: Retrying an applied range is an idempotent no-op.
//! - INV-8: No payload outside legal set or sealed-only mode.

use kamish_audit::AuditAction;
use kamish_enforce::{EnforcementRequest, Enforcer};
use kamish_log::{LogError, PartitionLog, RecordEnvelope, SessionError, SessionManager};
use kamish_meta::{MetadataStore, PolicyEngine, RegionId, ResidencyPolicy, StreamEvent};
use kamish_outbox::{BackpressureRule, DestinationOutbox, OutboxRange};
use kamish_policy::DestinationMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("Log error: {0}")]
    Log(#[from] LogError),

    #[error("Session error: {0}")]
    Session(#[from] SessionError),

    #[error("Policy denied operation: {0}")]
    PolicyDenied(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Helper function to parse region string into strongly-typed RegionId.
pub fn parse_region_id(s: &str) -> RegionId {
    match s.to_uppercase().as_str() {
        "US_EAST" | "REGION-US" | "US" => RegionId::UsEast,
        "EU_WEST" | "REGION-EU" | "EU" => RegionId::EuWest,
        "AP_SOUTH" | "REGION-AP" | "AP" => RegionId::ApSouth,
        other => RegionId::Custom(other.to_string()),
    }
}

/// Request to produce a batch of records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProduceRequest {
    pub topic: String,
    pub partition: u32,
    pub producer_id: u64,
    pub producer_epoch: u64,
    pub sequence_base: u32,
    pub class_id: String,
    pub payload: Vec<u8>,
    #[serde(default)]
    pub residency_rule: Option<ResidencyPolicy>,
}

/// Acknowledgment response for produce request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProduceResponse {
    pub base_offset: u64,
    pub count: u32,
    pub committed_epoch: u64,
    pub sovereignty_allowed: bool,
    pub denial_reason: String,
}

/// Request to fetch records from a partition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchRequest {
    pub topic: String,
    pub partition: u32,
    pub class_id: String,
    pub start_offset: u64,
    pub max_records: usize,
}

/// Response containing fetched records bounded by High-Water Mark (INV-4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResponse {
    pub high_watermark: u64,
    pub records: Vec<RecordEnvelope>,
}

/// Cross-region replication request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationRequest {
    pub topic: String,
    pub partition: u32,
    pub source_region: String,
    pub target_region: String,
    pub class_id: String,
    pub epoch: u64,
    pub from_offset: u64,
    pub to_offset: u64,
    pub payload: Vec<u8>,
    pub is_sealed: bool,
}

/// Response for replication transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationResponse {
    pub accepted: bool,
    pub applied_watermark: u64,
    pub reason: String,
}

/// Unified wire protocol request enum for async framed transport (§3.5.1, ADR-0005).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload")]
pub enum BrokerRequest {
    Produce(ProduceRequest),
    Fetch(FetchRequest),
    Replicate(ReplicationRequest),
    GetWatermark { topic: String, partition: u32, source_region: String },
    Ping,
}

/// Unified wire protocol response enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "payload")]
pub enum BrokerResponse {
    Produce(ProduceResponse),
    Fetch(FetchResponse),
    Replicate(ReplicationResponse),
    Watermark { durable_watermark: u64 },
    Pong,
    Error(String),
}

/// Regional Broker Server holding partition logs, metadata, outboxes, and enforcer.
#[derive(Clone)]
pub struct BrokerServer {
    pub region: String,
    pub region_id: RegionId,
    pub enforcer: Arc<Enforcer>,
    pub meta: Arc<MetadataStore>,
    pub policy_engine: Option<Arc<dyn PolicyEngine>>,
    pub known_peers: Vec<String>,
    data_dir: PathBuf,
    logs: Arc<RwLock<HashMap<(String, u32), PartitionLog>>>,
    sessions: Arc<RwLock<HashMap<(String, u32), SessionManager>>>,
    outboxes: Arc<RwLock<HashMap<String, DestinationOutbox>>>,
}

impl BrokerServer {
    pub fn new<P: AsRef<Path>>(
        region: impl Into<String>,
        enforcer: Arc<Enforcer>,
        meta: Arc<MetadataStore>,
        known_peers: Vec<String>,
        data_dir: P,
    ) -> Self {
        let region_str = region.into();
        let region_id = parse_region_id(&region_str);
        let data_dir_buf = data_dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&data_dir_buf);

        let mut outboxes_map = HashMap::new();
        for peer in &known_peers {
            outboxes_map.insert(
                peer.clone(),
                DestinationOutbox::new(peer, 5_000_000, BackpressureRule::Reject),
            );
        }

        Self {
            region: region_str,
            region_id,
            enforcer,
            meta,
            policy_engine: None,
            known_peers,
            data_dir: data_dir_buf,
            logs: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            outboxes: Arc::new(RwLock::new(outboxes_map)),
        }
    }

    /// Attaches a concrete PolicyEngine instance for ingress and egress policy enforcement (§3.B).
    pub fn with_policy_engine(mut self, engine: Arc<dyn PolicyEngine>) -> Self {
        self.policy_engine = Some(engine);
        self
    }

    /// Dynamically sets or updates the policy engine hook.
    pub fn set_policy_engine(&mut self, engine: Arc<dyn PolicyEngine>) {
        self.policy_engine = Some(engine);
    }

    /// Handles record ingestion with Ingress Interceptor Hooks, E1/E2 policy enforcement,
    /// deduplication, disk persistence, and outbox egress replication planning.
    pub fn produce(&self, req: ProduceRequest) -> Result<ProduceResponse, BrokerError> {
        // 1. Resolve canonical StreamEvent for the PolicyEngine trait contract
        let residency = req.residency_rule.clone().unwrap_or_else(|| {
            if req.class_id.contains("strict") || req.class_id.contains("local") {
                ResidencyPolicy::StrictLocal
            } else {
                ResidencyPolicy::GlobalCleartext
            }
        });

        let stream_event = StreamEvent::new(
            req.sequence_base as u64,
            &req.topic,
            format!("{}:{}", req.topic, req.partition),
            self.region_id.clone(),
            residency,
            req.payload.clone(),
        );

        // --- INGRESS INTERCEPTOR HOOK: Call PolicyEngine (§3.B) ---
        if let Some(ref engine) = self.policy_engine {
            if let Err(violation) = engine.validate_ingestion(&stream_event, &self.region_id) {
                tracing::warn!(
                    topic = %req.topic,
                    region = %self.region,
                    error = %violation,
                    "Ingress interceptor hook REJECTED event"
                );
                return Ok(ProduceResponse {
                    base_offset: 0,
                    count: 0,
                    committed_epoch: 0,
                    sovereignty_allowed: false,
                    denial_reason: violation.to_string(),
                });
            }
        }

        // 2. E1 & E2 Sovereign Policy Check (BrokerAppend / Ingress)
        let enf_req = EnforcementRequest::new(
            AuditAction::BrokerAppend,
            &req.class_id,
            &self.region,
            &self.region,
        );
        let decision = self.enforcer.decide(enf_req);
        if !decision.allowed {
            return Ok(ProduceResponse {
                base_offset: 0,
                count: 0,
                committed_epoch: 0,
                sovereignty_allowed: false,
                denial_reason: decision.reason,
            });
        }

        let epoch = 1; // Default partition epoch
        let session_mgr = self.get_or_create_session(&req.topic, req.partition);

        // 3. Idempotent Producer Session Deduplication (INV-6, FR-02)
        if let Some(cached_offset) = session_mgr.check_sequence(
            req.producer_id,
            req.producer_epoch,
            req.sequence_base,
        )? {
            return Ok(ProduceResponse {
                base_offset: cached_offset,
                count: 1,
                committed_epoch: epoch,
                sovereignty_allowed: true,
                denial_reason: "idempotent_duplicate".to_string(),
            });
        }

        // 4. Append to persistent segmented commit log with fsync durability (INV-1, INV-2)
        let log = self.get_or_create_log(&req.topic, req.partition, epoch)?;
        let offset = log.append(
            epoch,
            &req.class_id,
            decision.policy_version,
            req.payload.clone(),
        )?;

        // Update session state
        session_mgr.update(
            req.producer_id,
            req.producer_epoch,
            req.sequence_base,
            offset,
        );

        // 5. Cross-Region Replication Planning & EGRESS INTERCEPTOR HOOK (§3.B)
        for peer in &self.known_peers {
            let target_region_id = parse_region_id(peer);

            // --- EGRESS INTERCEPTOR HOOK: Call PolicyEngine (§3.B) ---
            if let Some(ref engine) = self.policy_engine {
                if let Err(violation) = engine.validate_replication(&stream_event, &self.region_id, &target_region_id) {
                    tracing::warn!(
                        topic = %req.topic,
                        source = %self.region,
                        target = %peer,
                        violation = %violation,
                        "Egress replication interceptor hook BLOCKED transfer"
                    );
                    continue; // Prohibited from outbox egress!
                }
            }

            let send_req = EnforcementRequest::new(
                AuditAction::ReplicationSender,
                &req.class_id,
                &self.region,
                peer,
            );
            let send_decision = self.enforcer.decide(send_req);
            if send_decision.allowed {
                let is_sealed = send_decision.mode == DestinationMode::SealedOnly;
                let outbox = self.get_or_create_outbox(peer);
                let range = OutboxRange {
                    topic: req.topic.clone(),
                    partition: req.partition,
                    epoch,
                    start_offset: offset,
                    end_offset: offset + 1,
                    class_id: req.class_id.clone(),
                    payload: req.payload.clone(),
                    is_sealed,
                };
                let _ = outbox.enqueue(range);
            }
        }

        Ok(ProduceResponse {
            base_offset: offset,
            count: 1,
            committed_epoch: epoch,
            sovereignty_allowed: true,
            denial_reason: "ok".to_string(),
        })
    }

    /// Handles consumer fetch requests with E7 policy enforcement, bounded by HWM (INV-4).
    pub fn fetch(&self, req: FetchRequest) -> Result<FetchResponse, BrokerError> {
        // E7: Consumer Fetch Policy Check
        let enf_req = EnforcementRequest::new(
            AuditAction::ConsumerFetch,
            &req.class_id,
            &self.region,
            &self.region,
        );
        let decision = self.enforcer.decide(enf_req);
        if !decision.allowed {
            return Err(BrokerError::PolicyDenied(decision.reason));
        }

        let log = self.get_or_create_log(&req.topic, req.partition, 1)?;
        let records = log.read(req.start_offset, req.max_records);
        Ok(FetchResponse {
            high_watermark: log.high_watermark(),
            records,
        })
    }

    /// Handles inbound cross-region replication with E5 policy enforcement and exact offset preservation (INV-5).
    pub fn handle_replication(&self, req: ReplicationRequest) -> Result<ReplicationResponse, BrokerError> {
        // E5: Replication Receiver Policy Check
        let enf_req = EnforcementRequest::new(
            AuditAction::ReplicationReceiver,
            &req.class_id,
            &req.source_region,
            &self.region,
        );
        let decision = self.enforcer.decide(enf_req);
        if !decision.allowed {
            return Ok(ReplicationResponse {
                accepted: false,
                applied_watermark: 0,
                reason: decision.reason,
            });
        }

        let log = self.get_or_create_log(&req.topic, req.partition, req.epoch)?;
        let local_next = log.next_offset();

        // Deduplication upon reconnect (INV-5, INV-6):
        // Case 1: Entire range is already committed locally -> Idempotent no-op ACK
        if req.to_offset <= local_next {
            return Ok(ReplicationResponse {
                accepted: true,
                applied_watermark: local_next,
                reason: "idempotent_duplicate_noop".to_string(),
            });
        }

        // Case 2: Exact next contiguous offset -> Append and advance watermark (INV-5)
        if req.from_offset == local_next {
            let applied_offset = log.append(
                req.epoch,
                &req.class_id,
                decision.policy_version,
                req.payload,
            )?;
            let new_watermark = applied_offset + 1;
            self.meta.update_watermark(&req.topic, req.partition, &req.source_region, new_watermark);

            return Ok(ReplicationResponse {
                accepted: true,
                applied_watermark: new_watermark,
                reason: "ok".to_string(),
            });
        }

        // Case 3: Gap detected (from_offset > local_next) -> Receiver reports current watermark
        Ok(ReplicationResponse {
            accepted: false,
            applied_watermark: local_next,
            reason: format!("offset_gap: expected {}, received {}", local_next, req.from_offset),
        })
    }

    /// Returns the durable watermark for a replicated stream from `source_region`.
    pub fn get_watermark(&self, topic: &str, partition: u32, source_region: &str) -> u64 {
        let meta_wm = self.meta.get_watermark(topic, partition, source_region);
        let log_wm = {
            let key = (topic.to_string(), partition);
            let logs = self.logs.read().unwrap();
            logs.get(&key).map(|l| l.next_offset()).unwrap_or(0)
        };
        std::cmp::max(meta_wm, log_wm)
    }

    /// Retrieves the durable outbox for a destination region.
    pub fn get_outbox(&self, target_region: &str) -> Option<DestinationOutbox> {
        self.outboxes.read().unwrap().get(target_region).cloned()
    }

    fn get_or_create_outbox(&self, target_region: &str) -> DestinationOutbox {
        let mut outboxes = self.outboxes.write().unwrap();
        outboxes
            .entry(target_region.to_string())
            .or_insert_with(|| {
                DestinationOutbox::new(target_region, 5_000_000, BackpressureRule::Reject)
            })
            .clone()
    }

    fn get_or_create_log(&self, topic: &str, partition: u32, epoch: u64) -> Result<PartitionLog, BrokerError> {
        let key = (topic.to_string(), partition);
        {
            let logs = self.logs.read().unwrap();
            if let Some(log) = logs.get(&key) {
                return Ok(log.clone());
            }
        }

        let mut logs = self.logs.write().unwrap();
        if let Some(log) = logs.get(&key) {
            return Ok(log.clone());
        }

        let part_dir = self.data_dir.join(format!("{}-{}", topic, partition));
        let log = PartitionLog::open(part_dir, epoch)?;
        logs.insert(key, log.clone());
        Ok(log)
    }

    fn get_or_create_session(&self, topic: &str, partition: u32) -> SessionManager {
        let key = (topic.to_string(), partition);
        {
            let sessions = self.sessions.read().unwrap();
            if let Some(s) = sessions.get(&key) {
                return s.clone();
            }
        }

        let mut sessions = self.sessions.write().unwrap();
        sessions.entry(key).or_insert_with(SessionManager::new).clone()
    }

    /// Dispatches a high-level request to the appropriate internal engine handler.
    pub fn dispatch(&self, req: BrokerRequest) -> BrokerResponse {
        match req {
            BrokerRequest::Produce(p) => match self.produce(p) {
                Ok(res) => BrokerResponse::Produce(res),
                Err(e) => BrokerResponse::Error(e.to_string()),
            },
            BrokerRequest::Fetch(f) => match self.fetch(f) {
                Ok(res) => BrokerResponse::Fetch(res),
                Err(e) => BrokerResponse::Error(e.to_string()),
            },
            BrokerRequest::Replicate(r) => match self.handle_replication(r) {
                Ok(res) => BrokerResponse::Replicate(res),
                Err(e) => BrokerResponse::Error(e.to_string()),
            },
            BrokerRequest::GetWatermark { topic, partition, source_region } => {
                let wm = self.get_watermark(&topic, partition, &source_region);
                BrokerResponse::Watermark { durable_watermark: wm }
            }
            BrokerRequest::Ping => BrokerResponse::Pong,
        }
    }

    /// Spawns an async framed TCP listener on `bind_addr` handling concurrent client connections.
    pub async fn start_tcp_listener(
        self: Arc<Self>,
        bind_addr: &str,
    ) -> Result<(std::net::SocketAddr, tokio::task::JoinHandle<()>), std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;

        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _peer_addr)) => {
                        let server = Arc::clone(&self);
                        tokio::spawn(async move {
                            let _ = server.handle_connection(stream).await;
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Accept error in broker TCP listener");
                        break;
                    }
                }
            }
        });

        Ok((local_addr, handle))
    }

    async fn handle_connection(&self, stream: TcpStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let req: BrokerRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = BrokerResponse::Error(format!("Malformed JSON request: {}", e));
                    let mut serialized = serde_json::to_vec(&err_resp)?;
                    serialized.push(b'\n');
                    writer.write_all(&serialized).await?;
                    writer.flush().await?;
                    continue;
                }
            };

            let resp = self.dispatch(req);
            let mut serialized = serde_json::to_vec(&resp)?;
            serialized.push(b'\n');
            writer.write_all(&serialized).await?;
            writer.flush().await?;
        }
        Ok(())
    }
}

/// Lightweight async framed TCP client connecting to a remote Kamish Broker daemon.
pub struct BrokerClient {
    lines: tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl BrokerClient {
    pub async fn connect(addr: impl tokio::net::ToSocketAddrs) -> Result<Self, std::io::Error> {
        let stream = TcpStream::connect(addr).await?;
        let (reader, writer) = stream.into_split();
        let lines = BufReader::new(reader).lines();
        Ok(Self { lines, writer })
    }

    pub async fn send_request(&mut self, req: BrokerRequest) -> Result<BrokerResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut payload = serde_json::to_vec(&req)?;
        payload.push(b'\n');
        self.writer.write_all(&payload).await?;
        self.writer.flush().await?;

        match self.lines.next_line().await? {
            Some(line) => {
                let resp: BrokerResponse = serde_json::from_str(&line)?;
                Ok(resp)
            }
            None => Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Connection closed by remote broker").into()),
        }
    }

    pub async fn produce(&mut self, req: ProduceRequest) -> Result<ProduceResponse, Box<dyn std::error::Error + Send + Sync>> {
        match self.send_request(BrokerRequest::Produce(req)).await? {
            BrokerResponse::Produce(p) => Ok(p),
            BrokerResponse::Error(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e).into()),
            other => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unexpected response: {:?}", other)).into()),
        }
    }

    pub async fn fetch(&mut self, req: FetchRequest) -> Result<FetchResponse, Box<dyn std::error::Error + Send + Sync>> {
        match self.send_request(BrokerRequest::Fetch(req)).await? {
            BrokerResponse::Fetch(f) => Ok(f),
            BrokerResponse::Error(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e).into()),
            other => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unexpected response: {:?}", other)).into()),
        }
    }

    pub async fn replicate(&mut self, req: ReplicationRequest) -> Result<ReplicationResponse, Box<dyn std::error::Error + Send + Sync>> {
        match self.send_request(BrokerRequest::Replicate(req)).await? {
            BrokerResponse::Replicate(r) => Ok(r),
            BrokerResponse::Error(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e).into()),
            other => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unexpected response: {:?}", other)).into()),
        }
    }

    pub async fn get_watermark(
        &mut self,
        topic: &str,
        partition: u32,
        source_region: &str,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        match self
            .send_request(BrokerRequest::GetWatermark {
                topic: topic.to_string(),
                partition,
                source_region: source_region.to_string(),
            })
            .await?
        {
            BrokerResponse::Watermark { durable_watermark } => Ok(durable_watermark),
            BrokerResponse::Error(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e).into()),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unexpected response: {:?}", other),
            )
            .into()),
        }
    }

    pub async fn ping(&mut self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        match self.send_request(BrokerRequest::Ping).await? {
            BrokerResponse::Pong => Ok(true),
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use kamish_audit::AuditLog;
    use kamish_meta::KamishPolicyEngine;
    use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy};
    use rand::rngs::OsRng;
    use std::collections::{HashMap, HashSet};

    fn setup_test_broker(local_region: &str, peers: Vec<String>) -> (BrokerServer, tempfile::TempDir) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert("region-eu".to_string());
        legal.insert("region-us".to_string());
        legal.insert("EU_WEST".to_string());
        legal.insert("US_EAST".to_string());

        let mut modes = HashMap::new();
        modes.insert("region-eu".to_string(), DestinationMode::Cleartext);
        modes.insert("region-us".to_string(), DestinationMode::Cleartext);
        modes.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
        modes.insert("US_EAST".to_string(), DestinationMode::Cleartext);

        // Class 1: Global allowed in EU and US
        let mut classes = HashMap::new();
        classes.insert(
            "global-data".to_string(),
            ClassRule {
                class_id: "global-data".to_string(),
                description: "Replicable".to_string(),
                legal_regions: legal.clone(),
                destination_modes: modes.clone(),
                ack_mode: AckMode::LocalMajority,
                failover_candidates: vec!["region-eu".to_string(), "region-us".to_string()],
                transit_allowed: true,
            },
        );

        // Class 2: Strict EU only
        let mut eu_only_legal = HashSet::new();
        eu_only_legal.insert("region-eu".to_string());
        eu_only_legal.insert("EU_WEST".to_string());
        let mut eu_modes = HashMap::new();
        eu_modes.insert("region-eu".to_string(), DestinationMode::Cleartext);
        eu_modes.insert("EU_WEST".to_string(), DestinationMode::Cleartext);
        eu_modes.insert("region-us".to_string(), DestinationMode::Deny);
        eu_modes.insert("US_EAST".to_string(), DestinationMode::Deny);

        classes.insert(
            "eu-strict".to_string(),
            ClassRule {
                class_id: "eu-strict".to_string(),
                description: "EU sovereignty strictly enforced".to_string(),
                legal_regions: eu_only_legal,
                destination_modes: eu_modes,
                ack_mode: AckMode::LocalMajority,
                failover_candidates: vec!["region-eu".to_string()],
                transit_allowed: false,
            },
        );

        let policy_doc = PolicyDocument {
            policy_id: "pol-test".to_string(),
            version: 1,
            issued_at_sec: Utc::now().timestamp() - 5,
            ttl_seconds: 3600,
            issuer_pubkey_hex: pubkey_hex.clone(),
            classes,
            transit_rules: HashMap::new(),
        };

        let signed = SignedPolicy::sign(policy_doc, &signing_key);
        let mut trusted = HashSet::new();
        trusted.insert(pubkey_hex);

        let mut cache = PolicyCache::new(trusted);
        cache.update(signed, Utc::now().timestamp()).unwrap();

        let audit_log = AuditLog::new_in_memory();
        let enforcer = Arc::new(Enforcer::new(local_region, Arc::new(RwLock::new(cache)), audit_log.clone()));
        let meta = Arc::new(MetadataStore::new());
        let temp_dir = tempfile::tempdir().unwrap();

        let mut broker = BrokerServer::new(local_region, enforcer, meta, peers, temp_dir.path());
        // Attach concrete KamishPolicyEngine for §3.B hooks
        let engine = Arc::new(KamishPolicyEngine::new(audit_log));
        broker.set_policy_engine(engine);

        (broker, temp_dir)
    }

    #[test]
    fn test_ingress_and_egress_interceptor_hooks() {
        let (broker_eu, _td) = setup_test_broker("EU_WEST", vec!["US_EAST".to_string()]);

        // 1. Ingress of StrictLocal in home region EU_WEST -> Passes ingress hook
        let req_strict = ProduceRequest {
            topic: "gdpr-records".to_string(),
            partition: 0,
            producer_id: 1101,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "eu-strict".to_string(),
            payload: b"eu-data".to_vec(),
            residency_rule: Some(ResidencyPolicy::StrictLocal),
        };
        let res = broker_eu.produce(req_strict).unwrap();
        assert!(res.sovereignty_allowed);
        assert_eq!(res.base_offset, 0);

        // 2. Verify EGRESS INTERCEPTOR HOOK:
        // Although US_EAST is a known peer, the egress hook blocked StrictLocal from entering the outbox!
        let us_outbox = broker_eu.get_outbox("US_EAST").unwrap();
        assert_eq!(us_outbox.next_transfer_batch(10).len(), 0);

        // 3. Ingest an event with RestrictedReplication to US_EAST
        let req_restricted = ProduceRequest {
            topic: "financial-stream".to_string(),
            partition: 0,
            producer_id: 1101,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".to_string(),
            payload: b"financial-data".to_vec(),
            residency_rule: Some(ResidencyPolicy::RestrictedReplication(vec![RegionId::UsEast])),
        };
        let res_restricted = broker_eu.produce(req_restricted).unwrap();
        assert!(res_restricted.sovereignty_allowed);

        // Egress hook permitted US_EAST: exactly 1 batch queued!
        assert_eq!(us_outbox.next_transfer_batch(10).len(), 1);
    }

    #[test]
    fn test_broker_produce_fetch_and_deduplication() {
        let (broker, _td) = setup_test_broker("region-eu", vec!["region-us".to_string()]);

        // 1. Produce record seq 0
        let req = ProduceRequest {
            topic: "orders".to_string(),
            partition: 0,
            producer_id: 1001,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".to_string(),
            payload: b"order-payload-1".to_vec(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        };
        let res = broker.produce(req.clone()).unwrap();
        assert!(res.sovereignty_allowed);
        assert_eq!(res.base_offset, 0);

        // 2. Retry seq 0 -> returns cached offset 0 without appending new entry (INV-6)
        let dup_res = broker.produce(req).unwrap();
        assert!(dup_res.sovereignty_allowed);
        assert_eq!(dup_res.base_offset, 0);
        assert_eq!(dup_res.denial_reason, "idempotent_duplicate");

        // 3. Fetch records up to HWM (INV-4)
        let fetch_req = FetchRequest {
            topic: "orders".to_string(),
            partition: 0,
            class_id: "global-data".to_string(),
            start_offset: 0,
            max_records: 10,
        };
        let fetch_res = broker.fetch(fetch_req).unwrap();
        assert_eq!(fetch_res.high_watermark, 1);
        assert_eq!(fetch_res.records.len(), 1);
        assert_eq!(fetch_res.records[0].payload, b"order-payload-1");
    }

    #[test]
    fn test_broker_strict_residency_rejection() {
        let (broker_us, _td) = setup_test_broker("region-us", vec!["region-eu".to_string()]);

        // Attempting to ingest EU-only restricted data into US region -> Rejected (E1/E2, INV-8)
        let req = ProduceRequest {
            topic: "medical-records".to_string(),
            partition: 0,
            producer_id: 2002,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "eu-strict".to_string(),
            payload: b"sensitive-data".to_vec(),
            residency_rule: Some(ResidencyPolicy::StrictLocal),
        };
        let res = broker_us.produce(req).unwrap();
        assert!(!res.sovereignty_allowed);
    }

    #[test]
    fn test_broker_outbox_replication_filter() {
        let (broker_eu, _td) = setup_test_broker("region-eu", vec!["region-us".to_string()]);

        // Ingest global-data -> enqueued to US outbox
        let req_global = ProduceRequest {
            topic: "telemetry".to_string(),
            partition: 0,
            producer_id: 3001,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".to_string(),
            payload: b"global-event".to_vec(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        };
        broker_eu.produce(req_global).unwrap();

        let us_outbox = broker_eu.get_outbox("region-us").unwrap();
        let batches = us_outbox.next_transfer_batch(10);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].payload, b"global-event");

        // Ingest eu-strict data -> ACCEPTED locally in EU, but NOT enqueued to US outbox (E4, INV-8)
        let req_strict = ProduceRequest {
            topic: "gdpr-telemetry".to_string(),
            partition: 0,
            producer_id: 3001,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "eu-strict".to_string(),
            payload: b"eu-only-event".to_vec(),
            residency_rule: Some(ResidencyPolicy::StrictLocal),
        };
        let res = broker_eu.produce(req_strict).unwrap();
        assert!(res.sovereignty_allowed);

        // US outbox still only has the 1 global record, 0 eu-strict records!
        let batches_after = us_outbox.next_transfer_batch(10);
        assert_eq!(batches_after.len(), 1);
    }

    #[test]
    fn test_multi_broker_replication_and_watermark_catchup() {
        let (broker_a, _td_a) = setup_test_broker("region-eu", vec!["region-us".to_string()]);
        let (broker_b, _td_b) = setup_test_broker("region-us", vec!["region-eu".to_string()]);

        // 1. Ingest 3 events into Broker A
        for seq in 0..3 {
            let req = ProduceRequest {
                topic: "orders".to_string(),
                partition: 0,
                producer_id: 5001,
                producer_epoch: 1,
                sequence_base: seq,
                class_id: "global-data".to_string(),
                payload: format!("order-{}", seq).into_bytes(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            };
            let res = broker_a.produce(req).unwrap();
            assert!(res.sovereignty_allowed);
            assert_eq!(res.base_offset, seq as u64);
        }

        // 2. Replication Agent steps: read from outbox, transmit to Broker B
        let outbox_a = broker_a.get_outbox("region-us").unwrap();
        let pending = outbox_a.next_transfer_batch(10);
        assert_eq!(pending.len(), 3);

        for range in pending {
            let rep_req = ReplicationRequest {
                topic: range.topic,
                partition: range.partition,
                source_region: "region-eu".to_string(),
                target_region: "region-us".to_string(),
                class_id: range.class_id,
                epoch: range.epoch,
                from_offset: range.start_offset,
                to_offset: range.end_offset,
                payload: range.payload,
                is_sealed: range.is_sealed,
            };
            let rep_res = broker_b.handle_replication(rep_req).unwrap();
            assert!(rep_res.accepted);
            outbox_a.advance_watermark(rep_res.applied_watermark);
        }

        // 3. Verify Broker A outbox is fully drained (watermark = 3)
        assert_eq!(outbox_a.durable_watermark(), 3);
        assert_eq!(outbox_a.next_transfer_batch(10).len(), 0);

        // 4. Verify Broker B consumer can read all 3 replicated records with exact matching offsets (INV-5)
        let fetch_req = FetchRequest {
            topic: "orders".to_string(),
            partition: 0,
            class_id: "global-data".to_string(),
            start_offset: 0,
            max_records: 10,
        };
        let fetch_res = broker_b.fetch(fetch_req).unwrap();
        assert_eq!(fetch_res.records.len(), 3);
        assert_eq!(fetch_res.records[0].base_offset, 0);
        assert_eq!(fetch_res.records[0].payload, b"order-0");
        assert_eq!(fetch_res.records[1].base_offset, 1);
        assert_eq!(fetch_res.records[1].payload, b"order-1");
        assert_eq!(fetch_res.records[2].base_offset, 2);
        assert_eq!(fetch_res.records[2].payload, b"order-2");
    }

    #[test]
    fn test_regional_outage_buffering_and_recovery() {
        let (broker_a, _td_a) = setup_test_broker("region-eu", vec!["region-us".to_string()]);
        let (broker_b, _td_b) = setup_test_broker("region-us", vec!["region-eu".to_string()]);

        // Broker B is "offline" (simulate WAN partition).
        for seq in 0..5 {
            let req = ProduceRequest {
                topic: "sensor-stream".to_string(),
                partition: 0,
                producer_id: 6001,
                producer_epoch: 1,
                sequence_base: seq,
                class_id: "global-data".to_string(),
                payload: format!("reading-{}", seq).into_bytes(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            };
            let res = broker_a.produce(req).unwrap();
            assert!(res.sovereignty_allowed);
        }

        let outbox_a = broker_a.get_outbox("region-us").unwrap();
        assert_eq!(outbox_a.next_transfer_batch(10).len(), 5);
        assert_eq!(outbox_a.durable_watermark(), 0);

        // Network heals: Broker B comes back online and accepts replication
        for range in outbox_a.next_transfer_batch(10) {
            let rep_req = ReplicationRequest {
                topic: range.topic,
                partition: range.partition,
                source_region: "region-eu".to_string(),
                target_region: "region-us".to_string(),
                class_id: range.class_id,
                epoch: range.epoch,
                from_offset: range.start_offset,
                to_offset: range.end_offset,
                payload: range.payload,
                is_sealed: range.is_sealed,
            };
            let rep_res = broker_b.handle_replication(rep_req).unwrap();
            assert!(rep_res.accepted);
            outbox_a.advance_watermark(rep_res.applied_watermark);
        }

        assert_eq!(outbox_a.durable_watermark(), 5);
        assert_eq!(outbox_a.next_transfer_batch(10).len(), 0);

        let fetch_res = broker_b.fetch(FetchRequest {
            topic: "sensor-stream".to_string(),
            partition: 0,
            class_id: "global-data".to_string(),
            start_offset: 0,
            max_records: 10,
        }).unwrap();
        assert_eq!(fetch_res.records.len(), 5);
        assert_eq!(fetch_res.records[4].base_offset, 4);
        assert_eq!(fetch_res.records[4].payload, b"reading-4");
    }

    #[tokio::test]
    async fn test_async_framed_tcp_listener_and_client() {
        let (broker, _td) = setup_test_broker("region-eu", vec!["region-us".to_string()]);
        let broker_arc = Arc::new(broker);

        let (local_addr, _handle) = match broker_arc.start_tcp_listener("127.0.0.1:0").await {
            Ok(res) => res,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Skipping TCP socket test in restricted sandbox: {}", e);
                return;
            }
            Err(e) => panic!("Failed to bind TCP listener: {}", e),
        };

        let mut client = match BrokerClient::connect(local_addr).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Skipping TCP socket test in restricted sandbox: {}", e);
                return;
            }
            Err(e) => panic!("Failed to connect to TCP listener: {}", e),
        };

        assert!(client.ping().await.unwrap());

        let prod_res = client.produce(ProduceRequest {
            topic: "tcp-stream".to_string(),
            partition: 0,
            producer_id: 777,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".to_string(),
            payload: b"tcp-payload".to_vec(),
            residency_rule: Some(ResidencyPolicy::GlobalCleartext),
        }).await.unwrap();
        assert!(prod_res.sovereignty_allowed);
        assert_eq!(prod_res.base_offset, 0);

        let fetch_res = client.fetch(FetchRequest {
            topic: "tcp-stream".to_string(),
            partition: 0,
            class_id: "global-data".to_string(),
            start_offset: 0,
            max_records: 5,
        }).await.unwrap();
        assert_eq!(fetch_res.records.len(), 1);
        assert_eq!(fetch_res.records[0].payload, b"tcp-payload");
    }
}
