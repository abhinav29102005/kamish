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
use kamish_log::{
    LogError, PartitionLog, RecordContext, RecordEnvelope, SessionError, SessionManager,
};
use kamish_meta::{
    DeploymentMode, LocalReplicationTracker, MetadataAuthority, MetadataStore, MonotonicClock,
    PartitionOwnership, PolicyEngine, RegionId, ResidencyPolicy, StreamEvent, StreamId,
    SystemMonotonicClock,
};
use kamish_outbox::{BackpressureRule, DestinationOutbox, OutboxError, OutboxRange};
use kamish_own::{OwnershipError, QuarantineStore};
use kamish_policy::DestinationMode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
pub mod catalog;
pub mod config;
pub mod tls;
pub use catalog::{TenantCatalog, TopicMetadata};
pub use config::{BrokerConfig, ConfigError, ServerLimits, TlsConfig, TopicConfig};
use std::pin::Pin;
use thiserror::Error;
pub use tls::{
    ClientIdentity, TlsError, build_client_config, build_server_config, extract_client_identity,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("Log error: {0}")]
    Log(#[from] LogError),

    #[error("Session error: {0}")]
    Session(#[from] SessionError),

    #[error("Policy denied operation: {0}")]
    PolicyDenied(String),

    #[error("Outbox error: {0}")]
    Outbox(#[from] OutboxError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Epoch fenced: current {current}, attempted {attempted}")]
    EpochFenced { current: u64, attempted: u64 },

    #[error("Authority error: {0}")]
    Authority(String),

    #[error("Ownership error: {0}")]
    Ownership(#[from] OwnershipError),

    #[error("Broker data directory {0:?} is locked by another running process (W14)")]
    DataDirectoryLocked(PathBuf),

    #[error("Payload size {size} exceeds maximum allowed payload limit {max}")]
    PayloadTooLarge { size: usize, max: usize },

    #[error("Frame size {size} exceeds maximum allowed frame limit {max}")]
    FrameTooLarge { size: usize, max: usize },

    #[error("Connection limit reached: server rejected connection (max {0})")]
    ConnectionLimitReached(usize),
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

/// Durable consumer-group offset commit request (W22).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitOffsetRequest {
    pub group_id: String,
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
}

/// Consumer-group offset fetch request (W22).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchOffsetRequest {
    pub group_id: String,
    pub topic: String,
    pub partition: u32,
}

/// Unified wire protocol request enum for async framed transport (§3.5.1, ADR-0005).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload")]
pub enum BrokerRequest {
    Produce(ProduceRequest),
    Fetch(FetchRequest),
    Replicate(ReplicationRequest),
    GetWatermark {
        topic: String,
        partition: u32,
        source_region: String,
    },
    CommitOffset(CommitOffsetRequest),
    FetchOffset(FetchOffsetRequest),
    PromotePartition {
        topic: String,
        partition: u32,
        class_id: Option<String>,
        required_watermark: Option<u64>,
    },
    QuarantineDivergentSuffix {
        topic: String,
        partition: u32,
        agreed_watermark: u64,
    },
    Ping,
}

/// Unified wire protocol response enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "payload")]
pub enum BrokerResponse {
    Produce(ProduceResponse),
    Fetch(FetchResponse),
    Replicate(ReplicationResponse),
    Watermark {
        durable_watermark: u64,
    },
    OffsetCommitted {
        group_id: String,
        topic: String,
        partition: u32,
        offset: u64,
    },
    OffsetFetched {
        group_id: String,
        topic: String,
        partition: u32,
        offset: Option<u64>,
    },
    PartitionPromoted {
        topic: String,
        partition: u32,
        owner_region: String,
        epoch: u64,
        lease_expiry_ns: i64,
    },
    SuffixQuarantined {
        topic: String,
        partition: u32,
        quarantined_count: usize,
    },
    Pong,
    Error(String),
}

use std::hash::{Hash, Hasher};

const NUM_STRIPES: usize = 32;

/// Lock-striped concurrent map eliminating cross-partition lock contention.
pub struct StripedStore<K, V> {
    stripes: [RwLock<HashMap<K, V>>; NUM_STRIPES],
}

impl<K: Eq + Hash + Clone, V: Clone> Default for StripedStore<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Clone, V: Clone> StripedStore<K, V> {
    pub fn new() -> Self {
        Self {
            stripes: std::array::from_fn(|_| RwLock::new(HashMap::new())),
        }
    }

    fn get_stripe_index(&self, key: &K) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % NUM_STRIPES
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let idx = self.get_stripe_index(key);
        let guard = self.stripes[idx].read().unwrap();
        guard.get(key).cloned()
    }

    pub fn keys(&self) -> Vec<K> {
        self.stripes
            .iter()
            .flat_map(|stripe| stripe.read().unwrap().keys().cloned().collect::<Vec<_>>())
            .collect()
    }

    pub fn get_or_insert_with<F: FnOnce() -> V>(&self, key: K, default_fn: F) -> V {
        let idx = self.get_stripe_index(&key);
        {
            let guard = self.stripes[idx].read().unwrap();
            if let Some(v) = guard.get(&key) {
                return v.clone();
            }
        }
        let mut guard = self.stripes[idx].write().unwrap();
        guard.entry(key).or_insert_with(default_fn).clone()
    }

    pub fn get_or_try_insert_with<E>(
        &self,
        key: K,
        create: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
        let idx = self.get_stripe_index(&key);
        let mut guard = self.stripes[idx].write().unwrap();
        if let Some(value) = guard.get(&key) {
            return Ok(value.clone());
        }
        // Exactly one initializer may recover/open a given physical log.
        let value = create()?;
        guard.insert(key, value.clone());
        Ok(value)
    }
}

type PartitionLock = Arc<Mutex<()>>;

type ConsumerCheckpoints = Arc<RwLock<HashMap<(String, String, u32), u64>>>;

/// Regional Broker Server holding partition logs, metadata, outboxes, and enforcer.
#[derive(Clone)]
pub struct BrokerServer {
    pub region: String,
    pub region_id: RegionId,
    pub enforcer: Arc<Enforcer>,
    pub meta: Arc<dyn MetadataAuthority>,
    pub local_tracker: Arc<dyn LocalReplicationTracker>,
    pub policy_engine: Option<Arc<dyn PolicyEngine>>,
    pub known_peers: Vec<String>,
    pub limits: ServerLimits,
    connection_semaphore: Arc<tokio::sync::Semaphore>,
    data_dir: PathBuf,
    logs: Arc<StripedStore<(String, u32), PartitionLog>>,
    sessions: Arc<StripedStore<(String, u32), SessionManager>>,
    partition_operations: Arc<StripedStore<(String, u32), PartitionLock>>,
    outboxes: Arc<RwLock<HashMap<String, DestinationOutbox>>>,
    pub clock: Arc<dyn MonotonicClock>,
    pub catalog: Arc<RwLock<TenantCatalog>>,
    pub quarantine: QuarantineStore,
    consumer_checkpoints: ConsumerCheckpoints,
    degraded_links: Arc<RwLock<HashSet<String>>>,
    owned_partitions: Arc<RwLock<HashSet<(String, u32)>>>,
    renewal_shutdown: Arc<tokio::sync::watch::Sender<bool>>,
}

impl BrokerServer {
    pub fn new<P: AsRef<Path>>(
        region: impl Into<String>,
        enforcer: Arc<Enforcer>,
        meta: Arc<dyn MetadataAuthority>,
        known_peers: Vec<String>,
        data_dir: P,
    ) -> Self {
        Self::new_with_tracker(
            region,
            enforcer,
            meta,
            Arc::new(MetadataStore::new()),
            known_peers,
            data_dir,
        )
    }

    pub fn new_with_tracker<P: AsRef<Path>>(
        region: impl Into<String>,
        enforcer: Arc<Enforcer>,
        meta: Arc<dyn MetadataAuthority>,
        local_tracker: Arc<dyn LocalReplicationTracker>,
        known_peers: Vec<String>,
        data_dir: P,
    ) -> Self {
        Self::try_new_with_tracker(region, enforcer, meta, local_tracker, known_peers, data_dir)
            .expect("broker storage initialization failed; use try_new to handle the error")
    }

    /// Opens existing partition logs and rebuilds bounded pending work before
    /// serving requests. Daemons must use this fallible constructor.
    pub fn try_new<P: AsRef<Path>>(
        region: impl Into<String>,
        enforcer: Arc<Enforcer>,
        meta: Arc<dyn MetadataAuthority>,
        known_peers: Vec<String>,
        data_dir: P,
    ) -> Result<Self, BrokerError> {
        Self::try_new_with_tracker(
            region,
            enforcer,
            meta,
            Arc::new(MetadataStore::new()),
            known_peers,
            data_dir,
        )
    }

    pub fn try_new_with_tracker<P: AsRef<Path>>(
        region: impl Into<String>,
        enforcer: Arc<Enforcer>,
        meta: Arc<dyn MetadataAuthority>,
        local_tracker: Arc<dyn LocalReplicationTracker>,
        known_peers: Vec<String>,
        data_dir: P,
    ) -> Result<Self, BrokerError> {
        let region_str = region.into();
        let region_id = parse_region_id(&region_str);
        let data_dir_buf = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir_buf)?;

        let limits = ServerLimits::default();
        let connection_semaphore = Arc::new(tokio::sync::Semaphore::new(limits.max_connections));

        let mut outboxes_map = HashMap::new();
        for peer in &known_peers {
            outboxes_map.insert(
                peer.clone(),
                DestinationOutbox::new(peer, 5_000_000, BackpressureRule::Reject),
            );
        }

        let catalog = Arc::new(RwLock::new(TenantCatalog::new()));
        let quarantine = QuarantineStore::open(data_dir_buf.join("quarantine"))?;
        let (renewal_tx, _) = tokio::sync::watch::channel(false);
        let broker = Self {
            region: region_str,
            region_id,
            enforcer,
            meta,
            local_tracker,
            policy_engine: None,
            known_peers,
            limits,
            connection_semaphore,
            data_dir: data_dir_buf,
            logs: Arc::new(StripedStore::new()),
            sessions: Arc::new(StripedStore::new()),
            partition_operations: Arc::new(StripedStore::new()),
            outboxes: Arc::new(RwLock::new(outboxes_map)),
            clock: Arc::new(SystemMonotonicClock::new()),
            catalog,
            quarantine,
            consumer_checkpoints: Arc::new(RwLock::new(HashMap::new())),
            degraded_links: Arc::new(RwLock::new(HashSet::new())),
            owned_partitions: Arc::new(RwLock::new(HashSet::new())),
            renewal_shutdown: Arc::new(renewal_tx),
        };

        // Load existing consumer checkpoints from disk (W22)
        let offsets_file = broker.data_dir.join("consumer_offsets.json");
        if let Ok(bytes) = std::fs::read(&offsets_file)
            && let Ok(map) = serde_json::from_slice::<HashMap<String, u64>>(&bytes)
        {
            let mut ckpt = broker.consumer_checkpoints.write().unwrap();
            for (k, v) in map {
                let parts: Vec<&str> = k.split(':').collect();
                if parts.len() == 3
                    && let Ok(p) = parts[2].parse::<u32>()
                {
                    ckpt.insert((parts[0].to_string(), parts[1].to_string(), p), v);
                }
            }
        }
        for entry in std::fs::read_dir(&broker.data_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if !entry.path().join("partition.data").exists() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let (topic, partition) = name
                .rsplit_once('-')
                .ok_or_else(|| std::io::Error::other("invalid stored partition directory"))?;
            let partition = partition
                .parse::<u32>()
                .map_err(|_| std::io::Error::other("invalid stored partition number"))?;
            let log = broker.get_or_create_log(topic, partition, 1)?;
            broker.get_or_create_session(topic, partition, &log);
        }
        for peer in &broker.known_peers {
            broker.refill_outbox(peer)?;
        }
        Ok(broker)
    }

    /// Sets the WAN degradation state for a peer region (W19).
    pub fn set_wan_degraded(&self, peer: &str, degraded: bool) {
        let mut links = self.degraded_links.write().unwrap();
        if degraded {
            links.insert(peer.to_string());
        } else {
            links.remove(peer);
        }
    }

    pub fn is_wan_degraded(&self, peer: &str) -> bool {
        self.degraded_links.read().unwrap().contains(peer)
    }

    /// Commits consumer group offset with crash-safe durable persistence (W22).
    pub fn commit_offset(&self, req: CommitOffsetRequest) -> Result<u64, BrokerError> {
        let mut checkpoints = self.consumer_checkpoints.write().unwrap();
        checkpoints.insert(
            (req.group_id.clone(), req.topic.clone(), req.partition),
            req.offset,
        );

        // Durable checkpoint persistence (W22)
        let offsets_file = self.data_dir.join("consumer_offsets.json");
        let tmp_file = self.data_dir.join("consumer_offsets.json.tmp");
        let serialized_map: HashMap<String, u64> = checkpoints
            .iter()
            .map(|((g, t, p), off)| (format!("{}:{}:{}", g, t, p), *off))
            .collect();
        if let Ok(bytes) = serde_json::to_vec(&serialized_map) {
            let _ = std::fs::write(&tmp_file, &bytes);
            let _ = std::fs::rename(&tmp_file, &offsets_file);
        }
        Ok(req.offset)
    }

    /// Fetches committed consumer group offset (W22).
    pub fn fetch_offset(&self, req: &FetchOffsetRequest) -> Option<u64> {
        self.consumer_checkpoints
            .read()
            .unwrap()
            .get(&(req.group_id.clone(), req.topic.clone(), req.partition))
            .copied()
    }
    /// Configures custom server limits (frame size, payload size, connection limit).
    pub fn with_limits(mut self, limits: ServerLimits) -> Self {
        self.connection_semaphore = Arc::new(tokio::sync::Semaphore::new(limits.max_connections));
        self.limits = limits;
        self
    }

    /// Attaches an authorized tenant and topic catalog (W10 / W13).
    pub fn with_catalog(self, catalog: TenantCatalog) -> Self {
        *self.catalog.write().unwrap() = catalog;
        self
    }

    /// Returns a reference-counted handle to the tenant catalog.
    pub fn catalog(&self) -> Arc<RwLock<TenantCatalog>> {
        Arc::clone(&self.catalog)
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

    /// Attaches an injectable monotonic clock interface.
    pub fn with_clock(mut self, clock: Arc<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }

    /// Claims or updates partition ownership via atomic CAS on the metadata authority.
    pub fn claim_partition_ownership(
        &self,
        topic: &str,
        partition: u32,
        expected_epoch: u64,
        new_epoch: u64,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, BrokerError> {
        let stream = StreamId::new(topic, partition);
        let ownership = self
            .meta
            .compare_and_swap_epoch(
                &stream,
                expected_epoch,
                new_epoch,
                &self.region,
                lease_duration_ms,
            )
            .map_err(|e| match e {
                kamish_meta::MetaError::StaleEpoch { current, proposed } => {
                    BrokerError::EpochFenced {
                        current,
                        attempted: proposed,
                    }
                }
                kamish_meta::MetaError::LeaseConflict { .. } => {
                    BrokerError::PolicyDenied("prior_owner_lease_active".into())
                }
                other => BrokerError::Authority(other.to_string()),
            })?;
        self.owned_partitions
            .write()
            .unwrap()
            .insert((topic.to_string(), partition));
        Ok(ownership)
    }

    /// Reacquires or renews local write authority for a persisted stream belonging to this region.
    pub fn reacquire_partition_ownership(
        &self,
        topic: &str,
        partition: u32,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, BrokerError> {
        let stream = StreamId::new(topic, partition);
        let ownership = self
            .meta
            .reacquire_ownership(&stream, &self.region, lease_duration_ms)
            .map_err(|e| match e {
                kamish_meta::MetaError::LeaseConflict { .. } => {
                    BrokerError::PolicyDenied("prior_owner_lease_active".into())
                }
                other => BrokerError::Authority(other.to_string()),
            })?;
        self.owned_partitions
            .write()
            .unwrap()
            .insert((topic.to_string(), partition));
        Ok(ownership)
    }

    /// Returns the currently owned partitions actively tracked for lease renewal.
    pub fn owned_partitions(&self) -> Vec<(String, u32)> {
        self.owned_partitions
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    /// Explicitly registers a partition into the proactive lease renewal set.
    pub fn register_owned_partition(&self, topic: &str, partition: u32) {
        self.owned_partitions
            .write()
            .unwrap()
            .insert((topic.to_string(), partition));
    }

    /// Explicitly unregisters a partition from proactive lease renewal.
    pub fn unregister_owned_partition(&self, topic: &str, partition: u32) {
        self.owned_partitions
            .write()
            .unwrap()
            .remove(&(topic.to_string(), partition));
    }

    /// Returns whether a partition is registered for active lease renewal.
    pub fn is_partition_renewal_active(&self, topic: &str, partition: u32) -> bool {
        self.owned_partitions
            .read()
            .unwrap()
            .contains(&(topic.to_string(), partition))
    }

    /// Starts the proactive background lease renewal loop complying with `docs/CONTRACT.md` §2.3.2.
    ///
    /// Periodically renews all partitions in `owned_partitions` on monotonic schedule:
    /// - `interval`: Renewal attempt interval (normative default: 3 seconds).
    /// - `timeout`: Per-attempt operation timeout (normative default: 1 second).
    /// - `lease_ttl_ms`: Target lease grant duration in milliseconds (normative default: 10_000 ms).
    ///
    /// Failed or timed-out renewals do NOT extend the local deadline. If a partition
    /// ownership is superseded or fenced, it is automatically evicted from active renewal.
    pub fn start_lease_renewal_worker(
        self: &Arc<Self>,
        interval: std::time::Duration,
        timeout: std::time::Duration,
        lease_ttl_ms: i64,
    ) -> tokio::task::JoinHandle<()> {
        let broker = Arc::clone(self);
        let mut shutdown_rx = broker.renewal_shutdown.subscribe();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let partitions = broker.owned_partitions();
                        if partitions.is_empty() {
                            continue;
                        }

                        for (topic, partition) in partitions {
                            let stream = StreamId::new(&topic, partition);
                            let meta = Arc::clone(&broker.meta);
                            let region = broker.region.clone();

                            let renewal_future = tokio::task::spawn_blocking(move || {
                                meta.reacquire_ownership(&stream, &region, lease_ttl_ms)
                            });

                            match tokio::time::timeout(timeout, renewal_future).await {
                                Ok(Ok(Ok(_ownership))) => {
                                    tracing::trace!(
                                        topic = %topic,
                                        partition = partition,
                                        "Proactive partition lease renewed successfully"
                                    );
                                }
                                Ok(Ok(Err(e))) => {
                                    tracing::warn!(
                                        topic = %topic,
                                        partition = partition,
                                        error = %e,
                                        "Partition lease renewal rejected or failed"
                                    );
                                    if matches!(
                                        e,
                                        kamish_meta::MetaError::LeaseConflict { .. }
                                            | kamish_meta::MetaError::PartitionNotFound { .. }
                                            | kamish_meta::MetaError::StaleEpoch { .. }
                                    ) {
                                        tracing::error!(
                                            topic = %topic,
                                            partition = partition,
                                            "Partition ownership was transferred or lost; removing from active renewal"
                                        );
                                        broker.unregister_owned_partition(&topic, partition);
                                    }
                                }
                                Ok(Err(join_err)) => {
                                    tracing::error!(
                                        topic = %topic,
                                        partition = partition,
                                        error = %join_err,
                                        "Lease renewal worker task panicked"
                                    );
                                }
                                Err(_elapsed) => {
                                    tracing::warn!(
                                        topic = %topic,
                                        partition = partition,
                                        timeout_ms = timeout.as_millis(),
                                        "Lease renewal timed out; retaining existing deadline without extension (§2.3.2)"
                                    );
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!(region = %broker.region, "Lease renewal worker shutting down");
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Signals the background lease renewal worker to terminate.
    pub fn stop_lease_renewal(&self) {
        let _ = self.renewal_shutdown.send(true);
    }

    /// Returns a reference to the local quarantine store (§1.7, §3.1.4).
    pub fn quarantine(&self) -> &QuarantineStore {
        &self.quarantine
    }

    /// Promotes this broker replica to become the authoritative partition owner following regional failover (§3.1.3, §3.4.4).
    ///
    /// Invariants & Lifecycle:
    /// 1. Requires `DeploymentMode::MultiRegion`.
    /// 2. Stream must be registered in metadata.
    /// 3. Existing owner lease must be expired + clock drift buffer (§4.7.2 R6, §3.4.4).
    /// 4. Candidate region must be eligible under policy E8 (`Enforcer::decide(AuditAction::Promote)`).
    /// 5. Candidate must hold an active replica log up to `required_watermark` if specified (INV-3).
    /// 6. Commits new epoch (`current_epoch + 1`) via atomic CAS on MetadataAuthority (INV-7) under 10s lease (CONTRACT.md §2.3.2).
    /// 7. Upon successful CAS, registers partition in proactive lease renewal.
    /// 8. If candidate's local log has divergent speculative records beyond `required_watermark`,
    ///    they are truncated from the active log and durably routed to `QuarantineStore`.
    /// 9. Fails closed on any error, never creating assumed uncommitted ownership.
    pub fn promote_partition(
        &self,
        topic: &str,
        partition: u32,
        class_id: Option<&str>,
        required_watermark: Option<u64>,
    ) -> Result<PartitionOwnership, BrokerError> {
        Self::validate_topic(topic)?;

        // 1. Must be in MultiRegion deployment mode
        if self.meta.deployment_mode() != DeploymentMode::MultiRegion {
            return Err(BrokerError::PolicyDenied(
                "promotion_requires_multi_region_mode".into(),
            ));
        }

        // 2. Partition must be registered in metadata
        let stream = StreamId::new(topic, partition);
        let current_ownership = match self.meta.get_ownership(topic, partition) {
            Some(o) => o,
            None => {
                return Err(BrokerError::PolicyDenied(
                    "unregistered_stream_cannot_be_promoted".into(),
                ));
            }
        };

        // If candidate already holds active valid lease for this partition, return current ownership
        let now_ns = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        if current_ownership.owner_region == self.region && current_ownership.is_lease_valid(now_ns) {
            return Ok(current_ownership);
        }

        // 3. Lease safety check with clock drift buffer (§4.7.2 R6, §3.4.4)
        let max_clock_drift_ms = 200i64;
        let safe_expiry_ns = current_ownership.lease_expiry_ns + (max_clock_drift_ms * 1_000_000);
        if now_ns < safe_expiry_ns {
            return Err(BrokerError::PolicyDenied(format!(
                "prior_owner_lease_active: expires at {}, current time is {}",
                safe_expiry_ns, now_ns
            )));
        }

        // 4. Resolve class_id and perform E8 Promotion Policy Check
        let resolved_class = match class_id {
            Some(c) => c.to_string(),
            None => {
                let catalog = self.catalog.read().unwrap();
                catalog
                    .get_topic(topic)
                    .and_then(|t| t.allowed_classes.iter().next().cloned())
                    .unwrap_or_else(|| "general".to_string())
            }
        };

        let enf_req = EnforcementRequest::new(
            AuditAction::Promote,
            &resolved_class,
            &current_ownership.owner_region,
            &self.region,
        );
        let decision = self.enforcer.decide(enf_req);
        if !decision.allowed {
            return Err(BrokerError::PolicyDenied(format!(
                "promotion_denied_by_policy: region '{}' not eligible for class '{}' ({})",
                self.region, resolved_class, decision.reason
            )));
        }

        // 5. Replica progress verification (INV-3)
        let operation = self
            .partition_operations
            .get_or_insert_with((topic.to_string(), partition), || {
                Arc::new(Mutex::new(()))
            });
        let _operation = operation.lock().unwrap();

        let log = self.get_or_create_log(topic, partition, current_ownership.epoch)?;
        let local_hwm = log.high_watermark();

        if let Some(req_wm) = required_watermark {
            if local_hwm < req_wm {
                return Err(BrokerError::PolicyDenied(format!(
                    "candidate_watermark_behind: candidate has {local_hwm}, required {req_wm}"
                )));
            }
        }

        // 6. Commit bumped epoch via atomic CAS on MetadataAuthority (INV-7)
        let new_epoch = current_ownership.epoch + 1;
        let lease_duration_ms = 10_000i64; // Normative 10-second lease (CONTRACT.md §2.3.2)
        let new_ownership = self.meta
            .compare_and_swap_epoch(
                &stream,
                current_ownership.epoch,
                new_epoch,
                &self.region,
                lease_duration_ms,
            )
            .map_err(|e| match e {
                kamish_meta::MetaError::StaleEpoch { current, proposed } => {
                    BrokerError::EpochFenced {
                        current,
                        attempted: proposed,
                    }
                }
                kamish_meta::MetaError::LeaseConflict { .. } => {
                    BrokerError::PolicyDenied("prior_owner_lease_active".into())
                }
                other => BrokerError::Authority(other.to_string()),
            })?;

        // 7. Update local ownership and active lease renewal registry
        self.owned_partitions
            .write()
            .unwrap()
            .insert((topic.to_string(), partition));

        // 8. If required_watermark was specified and candidate had speculative records beyond it,
        // isolate divergent records to QuarantineStore
        if let Some(req_wm) = required_watermark {
            if local_hwm > req_wm {
                let superseded = log.truncate_to_offset(req_wm)?;
                if !superseded.is_empty() {
                    tracing::warn!(
                        topic = %topic,
                        partition = partition,
                        count = superseded.len(),
                        agreed_watermark = req_wm,
                        "Quarantined speculative records beyond agreed frontier upon promotion"
                    );
                    self.quarantine.quarantine_records(superseded)?;
                }
            }
        }

        tracing::info!(
            topic = %topic,
            partition = partition,
            epoch = new_epoch,
            region = %self.region,
            "Successfully promoted broker replica to authoritative partition owner"
        );

        Ok(new_ownership)
    }

    /// Reconciles a local partition log against an agreed watermark / common prefix (§3.1.4, W20).
    ///
    /// If the local log contains records beyond `agreed_watermark`, those divergent suffix
    /// records are atomically truncated from the partition log and moved into the `QuarantineStore`.
    ///
    /// Returns the number of quarantined records.
    pub fn quarantine_divergent_suffix(
        &self,
        topic: &str,
        partition: u32,
        agreed_watermark: u64,
    ) -> Result<usize, BrokerError> {
        let operation = self
            .partition_operations
            .get_or_insert_with((topic.to_string(), partition), || {
                Arc::new(Mutex::new(()))
            });
        let _operation = operation.lock().unwrap();

        let log = self.get_or_create_log(topic, partition, 1)?;
        let local_hwm = log.high_watermark();

        if local_hwm <= agreed_watermark {
            return Ok(0);
        }

        let superseded = log.truncate_to_offset(agreed_watermark)?;
        let count = superseded.len();
        if !superseded.is_empty() {
            tracing::warn!(
                topic = %topic,
                partition = partition,
                count = count,
                agreed_watermark = agreed_watermark,
                "Quarantining divergent log suffix from superseded owner"
            );
            self.quarantine.quarantine_records(superseded)?;
        }

        // If this broker was previously renewing this partition, verify if we are still owner
        if let Some(ownership) = self.meta.get_ownership(topic, partition) {
            if ownership.owner_region != self.region {
                self.unregister_owned_partition(topic, partition);
            }
        }

        Ok(count)
    }

    /// Handles record ingestion without explicit client identity.
    pub fn produce(&self, req: ProduceRequest) -> Result<ProduceResponse, BrokerError> {
        self.produce_with_identity(req, None)
    }

    /// Handles record ingestion with Ingress Interceptor Hooks, E1/E2 policy enforcement,
    /// tenant/topic catalog checks (W10), deduplication, disk persistence, and outbox egress replication planning.
    pub fn produce_with_identity(
        &self,
        req: ProduceRequest,
        identity: Option<&ClientIdentity>,
    ) -> Result<ProduceResponse, BrokerError> {
        Self::validate_topic(&req.topic)?;
        if req.payload.len() > self.limits.max_payload_size {
            return Err(BrokerError::PayloadTooLarge {
                size: req.payload.len(),
                max: self.limits.max_payload_size,
            });
        }

        // W10: Catalog & tenant authorization checks (producers cannot choose another tenant's class)
        if let Some(topic_meta) = self.catalog.read().unwrap().get_topic(&req.topic) {
            if let Some(id) = identity {
                if !id.tenant.is_empty() && id.tenant != topic_meta.owner_tenant {
                    return Err(BrokerError::PolicyDenied(format!(
                        "Tenant mismatch: producer tenant '{}' cannot produce to topic '{}' owned by '{}' (W10)",
                        id.tenant, req.topic, topic_meta.owner_tenant
                    )));
                }
                if !topic_meta.authorized_producers.is_empty()
                    && !topic_meta.authorized_producers.contains(&id.principal)
                {
                    return Err(BrokerError::PolicyDenied(format!(
                        "Authorization denied: producer principal '{}' is not in authorized producers list for topic '{}' (W10)",
                        id.principal, req.topic
                    )));
                }
            }
            if !topic_meta.allowed_classes.contains(&req.class_id) {
                return Err(BrokerError::PolicyDenied(format!(
                    "Data class denied: class '{}' is not authorized for topic '{}' owned by tenant '{}' (W10)",
                    req.class_id, req.topic, topic_meta.owner_tenant
                )));
            }
        }

        let operation = self
            .partition_operations
            .get_or_insert_with((req.topic.clone(), req.partition), || {
                Arc::new(Mutex::new(()))
            });
        // Session check + durable append + session update + outbox publication
        // form one operation, not separate independently thread-safe steps.
        let _operation = operation.lock().unwrap();
        let topic = req.topic.clone();
        let partition = req.partition;

        let ownership = self.meta.get_ownership(&topic, partition);
        if self.meta.deployment_mode() == DeploymentMode::MultiRegion && ownership.is_none() {
            return Err(BrokerError::PolicyDenied(
                "unregistered_stream_in_multi_region_mode".into(),
            ));
        }

        if let Some(ref owner) = ownership {
            let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(i64::MAX);
            if owner.owner_region != self.region || now >= owner.lease_expiry_ns {
                return Err(BrokerError::PolicyDenied(
                    "not_current_partition_owner".into(),
                ));
            }
        }

        // Preserve single-node prototype compatibility: unregistered partition defaults to local authority at epoch 1.
        let epoch = ownership.as_ref().map_or(1, |owner| owner.epoch);
        let response = self.produce_at_epoch(req, epoch)?;
        if let Some(ref owner) = ownership {
            let post_ownership = self.meta.get_ownership(&topic, partition);
            let post_valid = match post_ownership {
                Some(ref post) => {
                    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(i64::MAX);
                    post.owner_region == self.region
                        && post.epoch == owner.epoch
                        && now < post.lease_expiry_ns
                }
                None => false,
            };
            if !post_valid {
                // Append may have committed; never send a successful ACK
                // after lease expiry. The client must resolve/retry identity.
                return Err(BrokerError::PolicyDenied(
                    "lease_expired_outcome_unknown".into(),
                ));
            }
        }
        Ok(response)
    }

    fn produce_at_epoch(
        &self,
        req: ProduceRequest,
        epoch: u64,
    ) -> Result<ProduceResponse, BrokerError> {
        // 1. Resolve canonical StreamEvent for the PolicyEngine trait contract
        // Class rules remain authoritative; an optional producer restriction may
        // only narrow them. Never infer authorization from class-name substrings.
        let residency = req
            .residency_rule
            .clone()
            .unwrap_or(ResidencyPolicy::GlobalCleartext);
        if matches!(residency, ResidencyPolicy::SealedCiphertext(_)) {
            return Err(BrokerError::PolicyDenied(
                "sealed_replication_not_configured".into(),
            ));
        }
        let request_policy =
            serde_json::to_string(&residency).map_err(|e| std::io::Error::other(e.to_string()))?;

        let stream_event = StreamEvent::new(
            req.sequence_base as u64,
            &req.topic,
            format!("{}:{}", req.topic, req.partition),
            self.region_id.clone(),
            residency,
            req.payload.clone(),
        );

        // --- INGRESS INTERCEPTOR HOOK: Call PolicyEngine (§3.B) ---
        #[allow(clippy::collapsible_if)]
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

        let log = self.get_or_create_log(&req.topic, req.partition, epoch)?;
        let session_mgr = self.get_or_create_session(&req.topic, req.partition, &log);

        // 3. Idempotent Producer Session Deduplication (INV-6, FR-02)
        if let Some(cached_offset) =
            session_mgr.check_sequence(req.producer_id, req.producer_epoch, req.sequence_base)?
        {
            let log = self.get_or_create_log(&req.topic, req.partition, epoch)?;
            let records = log.read(cached_offset, 1);
            if !records.first().is_some_and(|record| {
                record.payload == req.payload
                    && record.class_id == req.class_id
                    && record
                        .context
                        .as_ref()
                        .is_some_and(|ctx| ctx.request_policy == request_policy)
            }) {
                return Err(BrokerError::PolicyDenied(
                    "producer_sequence_content_conflict".into(),
                ));
            }
            return Ok(ProduceResponse {
                base_offset: cached_offset,
                count: 1,
                committed_epoch: epoch,
                sovereignty_allowed: true,
                denial_reason: "idempotent_duplicate".to_string(),
            });
        }

        if let Some(first) = log.read(0, 1).first()
            && (first.class_id != req.class_id
                || first
                    .context
                    .as_ref()
                    .is_some_and(|context| context.request_policy != request_policy))
        {
            return Ok(ProduceResponse {
                base_offset: 0,
                count: 0,
                committed_epoch: epoch,
                sovereignty_allowed: false,
                denial_reason: "stream_policy_change_requires_new_topic".into(),
            });
        }

        // 3.5. Outbox Capacity Admission Control & Backpressure Pre-check (§3.2.4, INV-3)
        // 3.5. Outbox Capacity Admission Control & Atomic Reservation (§3.2.4, INV-3)
        // Check policy and atomically reserve capacity in all permitted destination outboxes.
        // If ANY destination outbox has insufficient capacity, all reservations are rolled back
        // and the write is rejected BEFORE appending to the local commit log.
        let required_range_size = OutboxRange::required_capacity(req.payload.len());
        let mut reservations = Vec::new();

        for peer in &self.known_peers {
            let target_region_id = parse_region_id(peer);
            let producer_allows = match &stream_event.residency_rule {
                ResidencyPolicy::StrictLocal => target_region_id == self.region_id,
                ResidencyPolicy::GlobalCleartext => true,
                ResidencyPolicy::RestrictedReplication(regions)
                | ResidencyPolicy::SealedCiphertext(regions) => regions.contains(&target_region_id),
            };
            if !producer_allows {
                continue;
            }
            #[allow(clippy::collapsible_if)]
            if let Some(ref engine) = self.policy_engine {
                if engine
                    .validate_replication(&stream_event, &self.region_id, &target_region_id)
                    .is_err()
                {
                    continue; // Prohibited from outbox egress by policy
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
                // Fail closed until an authenticated encryption/key-custody
                // integration exists. A boolean flag is not encryption.
                if send_decision.mode == DestinationMode::SealedOnly {
                    return Err(BrokerError::PolicyDenied(
                        "sealed_replication_not_configured".into(),
                    ));
                }
                let outbox = self.get_or_create_outbox(peer);
                match outbox.reserve_capacity(required_range_size) {
                    Ok(reservation) => {
                        let is_sealed = send_decision.mode == DestinationMode::SealedOnly;
                        reservations.push((peer.clone(), reservation, is_sealed));
                    }
                    Err(OutboxError::QueueCapacityExceeded { current, max }) => {
                        tracing::warn!(
                            topic = %req.topic,
                            target = %peer,
                            current = current,
                            max = max,
                            "Replication outbox capacity exceeded; rejecting write with backpressure"
                        );
                        // Explicitly clear/drop any reservations acquired for previous peers
                        drop(reservations);
                        return Ok(ProduceResponse {
                            base_offset: 0,
                            count: 0,
                            committed_epoch: 0,
                            sovereignty_allowed: false,
                            denial_reason: format!(
                                "Backpressure: destination outbox for '{}' full",
                                peer
                            ),
                        });
                    }
                    Err(e) => {
                        return Err(BrokerError::Outbox(e));
                    }
                }
            }
        }

        // 4. Append to persistent segmented commit log with fsync durability (INV-1, INV-2)
        // Note: If log.append fails, `reservations` is dropped and capacity is safely rolled back via RAII.
        let log = self.get_or_create_log(&req.topic, req.partition, epoch)?;
        let context = RecordContext {
            producer_id: req.producer_id,
            producer_epoch: req.producer_epoch,
            sequence: req.sequence_base,
            origin_region: self.region.clone(),
            request_policy,
            replication_targets: reservations
                .iter()
                .map(|(peer, _, _)| peer.clone())
                .collect(),
            source_generation: self
                .meta
                .issue_source_identity(&format!("cluster-{}", self.region), &self.region)
                .map_or(1, |g| g.generation),
        };
        if let Some(first) = log.read(0, 1).first() {
            if let Some(ref first_ctx) = first.context {
                if first_ctx.origin_region == self.region {
                    let mut original = first_ctx.replication_targets.clone();
                    let mut proposed = context.replication_targets.clone();
                    original.sort();
                    proposed.sort();
                    if original != proposed {
                        return Ok(ProduceResponse {
                            base_offset: 0,
                            count: 0,
                            committed_epoch: epoch,
                            sovereignty_allowed: false,
                            denial_reason: "stream_destinations_changed_requires_new_topic".into(),
                        });
                    }
                }
            }
        }
        let offset = log.append_with_context(
            epoch,
            &req.class_id,
            decision.policy_version,
            req.payload.clone(),
            Some(context),
        )?;

        // Update session state
        session_mgr.update(
            req.producer_id,
            req.producer_epoch,
            req.sequence_base,
            offset,
        );

        // 5. Commit all reserved outbox ranges (§3.B)
        for (peer, reservation, is_sealed) in reservations {
            let range = OutboxRange {
                topic: req.topic.clone(),
                partition: req.partition,
                epoch,
                start_offset: offset,
                end_offset: offset + 1,
                class_id: req.class_id.clone(),
                payload: req.payload.clone(),
                is_sealed,
                source_generation: self
                    .meta
                    .issue_source_identity(&format!("cluster-{}", self.region), &self.region)
                    .map_or(1, |g| g.generation),
                envelope_payload: None,
            };
            if let Err(e) = reservation.commit(range) {
                tracing::warn!(
                    topic = %req.topic,
                    partition = req.partition,
                    offset = offset,
                    peer = %peer,
                    error = %e,
                    "Reservation commit growth exceeded outbox capacity post-append; obligation retained in durable WAL for backlog refill"
                );
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
        self.fetch_with_identity(req, None)
    }

    /// Consumer fetch bound to authenticated consumer identity, region, and role (W10 / W13).
    pub fn fetch_with_identity(
        &self,
        req: FetchRequest,
        identity: Option<&ClientIdentity>,
    ) -> Result<FetchResponse, BrokerError> {
        // W10 / W13: Catalog & tenant authorization checks
        if let Some(topic_meta) = self.catalog.read().unwrap().get_topic(&req.topic) {
            if let Some(id) = identity {
                if !id.tenant.is_empty() && id.tenant != topic_meta.owner_tenant {
                    return Err(BrokerError::PolicyDenied(format!(
                        "Tenant mismatch: consumer tenant '{}' cannot fetch from topic '{}' owned by '{}' (W10)",
                        id.tenant, req.topic, topic_meta.owner_tenant
                    )));
                }
                if !topic_meta.allowed_consumer_regions.is_empty()
                    && !topic_meta.allowed_consumer_regions.contains(&id.region)
                {
                    return Err(BrokerError::PolicyDenied(format!(
                        "Region boundary denied: consumer in region '{}' cannot fetch topic '{}' restricted to regions {:?} (W13)",
                        id.region, req.topic, topic_meta.allowed_consumer_regions
                    )));
                }
                if !topic_meta.authorized_consumers.is_empty()
                    && !topic_meta.authorized_consumers.contains(&id.principal)
                {
                    return Err(BrokerError::PolicyDenied(format!(
                        "Authorization denied: consumer principal '{}' is not in authorized consumers list for topic '{}' (W13)",
                        id.principal, req.topic
                    )));
                }
            }
            if !topic_meta.allowed_classes.contains(&req.class_id) {
                return Err(BrokerError::PolicyDenied(format!(
                    "Data class denied: class '{}' is not authorized for topic '{}' owned by tenant '{}' (W10)",
                    req.class_id, req.topic, topic_meta.owner_tenant
                )));
            }
        }

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
        // Caller-supplied class is not authority for the records on disk. Deny
        // the entire response if any actual record cannot be served; do not leak
        // a permitted prefix before discovering a later restricted record.
        for record in &records {
            if record.class_id != req.class_id {
                return Err(BrokerError::PolicyDenied("fetch_class_mismatch".into()));
            }
            let decision = self.enforcer.decide(EnforcementRequest::new(
                AuditAction::ConsumerFetch,
                &record.class_id,
                &self.region,
                &self.region,
            ));
            if !decision.allowed {
                return Err(BrokerError::PolicyDenied(decision.reason));
            }
        }
        Ok(FetchResponse {
            high_watermark: log.high_watermark(),
            records,
        })
    }

    /// Handles inbound cross-region replication with E5 policy enforcement and exact offset preservation (INV-5).
    pub fn handle_replication(
        &self,
        req: ReplicationRequest,
    ) -> Result<ReplicationResponse, BrokerError> {
        Self::validate_topic(&req.topic)?;
        let operation = self
            .partition_operations
            .get_or_insert_with((req.topic.clone(), req.partition), || {
                Arc::new(Mutex::new(()))
            });
        let _operation = operation.lock().unwrap();
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
        if req.is_sealed || decision.mode == DestinationMode::SealedOnly {
            return Ok(ReplicationResponse {
                accepted: false,
                applied_watermark: 0,
                reason: "sealed_replication_not_configured".into(),
            });
        }

        let log = self.get_or_create_log(&req.topic, req.partition, req.epoch)?;
        let local_next = log.next_offset();

        if req.target_region != self.region || req.epoch == 0 || req.epoch < log.current_epoch() {
            return Ok(ReplicationResponse {
                accepted: false,
                applied_watermark: local_next,
                reason: "replication_target_or_epoch_mismatch".into(),
            });
        }
        // The current wire envelope is one opaque record, not a record batch.
        // Never acknowledge a declared range that its payload cannot represent.
        if req.from_offset.checked_add(1) != Some(req.to_offset) {
            let reason = if req.from_offset < local_next && req.to_offset > local_next {
                format!(
                    "partial_overlap: local watermark is {local_next}; resume with one-record ranges"
                )
            } else {
                "invalid_replication_range: exactly one record is required".into()
            };
            return Ok(ReplicationResponse {
                accepted: false,
                applied_watermark: local_next,
                reason,
            });
        }

        // Deduplication upon reconnect (INV-5, INV-6):
        // Case 1: Entire range is already committed locally -> Idempotent no-op ACK
        if req.to_offset <= local_next {
            let same_record = log.read(req.from_offset, 1).first().is_some_and(|record| {
                record.epoch == req.epoch
                    && record.class_id == req.class_id
                    && record.payload == req.payload
                    && record.verify_checksum()
            });
            if !same_record {
                return Ok(ReplicationResponse {
                    accepted: false,
                    applied_watermark: local_next,
                    reason: "replication_history_conflict".into(),
                });
            }
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
            self.local_tracker.update_watermark(
                &req.topic,
                req.partition,
                &req.source_region,
                new_watermark,
            );

            return Ok(ReplicationResponse {
                accepted: true,
                applied_watermark: new_watermark,
                reason: "ok".to_string(),
            });
        }

        // Case 3: Gap or partial overlap detected
        if req.from_offset > local_next {
            Ok(ReplicationResponse {
                accepted: false,
                applied_watermark: local_next,
                reason: format!(
                    "offset_gap: expected {}, received {}",
                    local_next, req.from_offset
                ),
            })
        } else {
            // from_offset < local_next, but to_offset > local_next (partial overlap)
            Ok(ReplicationResponse {
                accepted: false,
                applied_watermark: local_next,
                reason: format!(
                    "partial_overlap: local watermark is {}, please resume from {}",
                    local_next, local_next
                ),
            })
        }
    }

    /// Returns the durable watermark for a replicated stream from `source_region`.
    pub fn get_watermark(&self, topic: &str, partition: u32, source_region: &str) -> u64 {
        let meta_wm = self
            .local_tracker
            .get_watermark(topic, partition, source_region);
        let log_wm = {
            let key = (topic.to_string(), partition);
            self.logs.get(&key).map(|l| l.next_offset()).unwrap_or(0)
        };
        std::cmp::max(meta_wm, log_wm)
    }

    /// Retrieves the durable outbox for a destination region.
    pub fn get_outbox(&self, target_region: &str) -> Option<DestinationOutbox> {
        self.outboxes.read().unwrap().get(target_region).cloned()
    }

    /// Sets the byte capacity for a specific destination outbox.
    pub fn set_outbox_capacity(&self, target_region: &str, max_bytes: usize) {
        let outbox = self.get_or_create_outbox(target_region);
        outbox.set_max_bytes(max_bytes);
    }

    /// Repopulates pending un-replicated records from the persistent disk log into the destination outbox.
    ///
    /// This ensures that after a broker restart or outage, un-replicated records are recovered
    /// from the durable commit log starting from the destination's durable watermark up to HWM.
    pub fn repopulate_outbox_from_log(
        &self,
        topic: &str,
        partition: u32,
        target_region: &str,
    ) -> Result<usize, BrokerError> {
        Self::validate_topic(topic)?;
        let operation = self
            .partition_operations
            .get_or_insert_with((topic.to_string(), partition), || Arc::new(Mutex::new(())));
        let _operation = operation.lock().unwrap();
        let outbox = self.get_or_create_outbox(target_region);
        let start_offset = outbox.durable_partition_watermark(topic, partition);

        let log = self.get_or_create_log(topic, partition, 1)?;
        let hwm = log.high_watermark();

        if start_offset >= hwm {
            return Ok(0);
        }

        let mut staged_count = 0;
        let mut next = start_offset;
        while next < hwm {
            for record in log.read(next, 128) {
                next = record.base_offset + 1;
                // Old envelopes did not preserve producer-supplied restrictions.
                // They cannot be reconstructed under a guessed class policy.
                let Some(context) = record.context.as_ref() else {
                    tracing::warn!(
                        topic,
                        partition,
                        offset = record.base_offset,
                        "Skipping outbound recovery of legacy/replica record without admission context"
                    );
                    continue;
                };
                if context.origin_region != self.region
                    || !context
                        .replication_targets
                        .iter()
                        .any(|r| r == target_region)
                {
                    continue;
                }
                if outbox.contains_partition_offset(topic, partition, record.base_offset) {
                    continue;
                }

                let send_req = EnforcementRequest::new(
                    AuditAction::ReplicationSender,
                    &record.class_id,
                    &self.region,
                    target_region,
                );
                let send_decision = self.enforcer.decide(send_req);
                if send_decision.allowed {
                    if send_decision.mode != DestinationMode::Cleartext {
                        continue; // Never reconstruct plaintext as a sealed range.
                    }
                    let range = OutboxRange {
                        topic: topic.to_string(),
                        partition,
                        epoch: record.epoch,
                        start_offset: record.base_offset,
                        end_offset: record.base_offset + 1,
                        class_id: record.class_id.clone(),
                        payload: record.payload.clone(),
                        is_sealed: false,
                        source_generation: record
                            .context
                            .as_ref()
                            .map_or(1, |c| c.source_generation),
                        envelope_payload: None,
                    };
                    match outbox.enqueue(range) {
                        Ok(()) => staged_count += 1,
                        Err(OutboxError::QueueCapacityExceeded { .. }) => return Ok(staged_count),
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }

        Ok(staged_count)
    }

    /// Refill after draining a bounded recovery page; no full-log payload clone.
    pub fn refill_outbox(&self, target_region: &str) -> Result<usize, BrokerError> {
        let mut count = 0;
        let mut streams = self.logs.keys();
        streams.sort();
        for (topic, partition) in streams {
            count += self.repopulate_outbox_from_log(&topic, partition, target_region)?;
        }
        Ok(count)
    }

    fn get_or_create_outbox(&self, target_region: &str) -> DestinationOutbox {
        {
            let outboxes = self.outboxes.read().unwrap();
            if let Some(ob) = outboxes.get(target_region) {
                return ob.clone();
            }
        }
        let mut outboxes = self.outboxes.write().unwrap();
        outboxes
            .entry(target_region.to_string())
            .or_insert_with(|| {
                DestinationOutbox::new(target_region, 5_000_000, BackpressureRule::Reject)
            })
            .clone()
    }

    fn get_or_create_log(
        &self,
        topic: &str,
        partition: u32,
        epoch: u64,
    ) -> Result<PartitionLog, BrokerError> {
        Self::validate_topic(topic)?;
        let key = (topic.to_string(), partition);
        if let Some(log) = self.logs.get(&key) {
            return Ok(log);
        }
        self.logs.get_or_try_insert_with(key, || {
            let part_dir = self.data_dir.join(format!("{}-{}", topic, partition));
            Ok(PartitionLog::open(part_dir, epoch)?)
        })
    }

    fn validate_topic(topic: &str) -> Result<(), BrokerError> {
        // Topic names are a single bounded path component, never a path. Keep
        // the existing on-disk naming scheme while rejecting traversal and
        // absolute paths before any directory creation/open.
        if topic.is_empty()
            || topic.len() > 249
            || topic == "."
            || topic == ".."
            || !topic
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(BrokerError::PolicyDenied("invalid_topic_name".into()));
        }
        Ok(())
    }

    fn get_or_create_session(
        &self,
        topic: &str,
        partition: u32,
        log: &PartitionLog,
    ) -> SessionManager {
        let key = (topic.to_string(), partition);
        self.sessions.get_or_insert_with(key, || {
            let sessions = SessionManager::new();
            let mut offset = 0;
            while offset < log.high_watermark() {
                for record in log.read(offset, 128) {
                    if let Some(context) = &record.context
                        && context.origin_region == self.region
                    {
                        sessions.update(
                            context.producer_id,
                            context.producer_epoch,
                            context.sequence,
                            record.base_offset,
                        );
                    }
                    offset = record.base_offset + 1;
                }
            }
            sessions
        })
    }

    /// Dispatches a high-level request without client identity.
    pub fn dispatch(&self, req: BrokerRequest) -> BrokerResponse {
        self.dispatch_with_identity(req, None)
    }

    /// Dispatches a high-level request bound to authenticated client identity (W09/W10/W13).
    pub fn dispatch_with_identity(
        &self,
        req: BrokerRequest,
        identity: Option<&ClientIdentity>,
    ) -> BrokerResponse {
        match req {
            BrokerRequest::Produce(p) => match self.produce_with_identity(p, identity) {
                Ok(res) => BrokerResponse::Produce(res),
                Err(e) => BrokerResponse::Error(e.to_string()),
            },
            BrokerRequest::Fetch(f) => match self.fetch_with_identity(f, identity) {
                Ok(res) => BrokerResponse::Fetch(res),
                Err(e) => BrokerResponse::Error(e.to_string()),
            },
            BrokerRequest::Replicate(r) => match self.handle_replication(r) {
                Ok(res) => BrokerResponse::Replicate(res),
                Err(e) => BrokerResponse::Error(e.to_string()),
            },
            BrokerRequest::GetWatermark {
                topic,
                partition,
                source_region,
            } => {
                let wm = self.get_watermark(&topic, partition, &source_region);
                BrokerResponse::Watermark {
                    durable_watermark: wm,
                }
            }
            BrokerRequest::CommitOffset(c) => {
                let group_id = c.group_id.clone();
                let topic = c.topic.clone();
                let partition = c.partition;
                match self.commit_offset(c) {
                    Ok(offset) => BrokerResponse::OffsetCommitted {
                        group_id,
                        topic,
                        partition,
                        offset,
                    },
                    Err(e) => BrokerResponse::Error(e.to_string()),
                }
            }
            BrokerRequest::FetchOffset(f) => {
                let offset = self.fetch_offset(&f);
                BrokerResponse::OffsetFetched {
                    group_id: f.group_id,
                    topic: f.topic,
                    partition: f.partition,
                    offset,
                }
            }
            BrokerRequest::PromotePartition {
                topic,
                partition,
                class_id,
                required_watermark,
            } => {
                match self.promote_partition(
                    &topic,
                    partition,
                    class_id.as_deref(),
                    required_watermark,
                ) {
                    Ok(ownership) => BrokerResponse::PartitionPromoted {
                        topic: ownership.topic,
                        partition: ownership.partition,
                        owner_region: ownership.owner_region,
                        epoch: ownership.epoch,
                        lease_expiry_ns: ownership.lease_expiry_ns,
                    },
                    Err(e) => BrokerResponse::Error(e.to_string()),
                }
            }
            BrokerRequest::QuarantineDivergentSuffix {
                topic,
                partition,
                agreed_watermark,
            } => match self.quarantine_divergent_suffix(&topic, partition, agreed_watermark) {
                Ok(quarantined_count) => BrokerResponse::SuffixQuarantined {
                    topic,
                    partition,
                    quarantined_count,
                },
                Err(e) => BrokerResponse::Error(e.to_string()),
            },
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
                    Ok((mut stream, _peer_addr)) => {
                        let permit =
                            match Arc::clone(&self.connection_semaphore).try_acquire_owned() {
                                Ok(p) => p,
                                Err(_) => {
                                    tracing::warn!(
                                        max = self.limits.max_connections,
                                        "Broker connection limit reached; rejecting connection"
                                    );
                                    let err_resp = BrokerResponse::Error(format!(
                                        "Server connection limit reached (max {})",
                                        self.limits.max_connections
                                    ));
                                    if let Ok(mut bytes) = serde_json::to_vec(&err_resp) {
                                        bytes.push(b'\n');
                                        let _ = stream.write_all(&bytes).await;
                                        let _ = stream.flush().await;
                                    }
                                    continue;
                                }
                            };
                        let server = Arc::clone(&self);
                        tokio::spawn(async move {
                            let _permit = permit;
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

    /// Spawns an async framed TLS 1.3 listener on `bind_addr` enforcing mutual authentication (W09).
    pub async fn start_tls_listener(
        self: Arc<Self>,
        bind_addr: &str,
        acceptor: tokio_rustls::TlsAcceptor,
    ) -> Result<(std::net::SocketAddr, tokio::task::JoinHandle<()>), std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;

        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _peer_addr)) => {
                        let permit =
                            match Arc::clone(&self.connection_semaphore).try_acquire_owned() {
                                Ok(p) => p,
                                Err(_) => {
                                    tracing::warn!(
                                        max = self.limits.max_connections,
                                        "Broker connection limit reached; rejecting connection"
                                    );
                                    continue;
                                }
                            };
                        let server = Arc::clone(&self);
                        let acceptor = acceptor.clone();
                        tokio::spawn(async move {
                            let _permit = permit;
                            // TLS 1.3 Handshake: unknown peers and invalid certs fail-closed (W09)
                            let tls_stream = match acceptor.accept(stream).await {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        "TLS 1.3 handshake failed; rejecting untrusted or unknown peer (W09)"
                                    );
                                    return;
                                }
                            };

                            // Extract verified client identity from peer certificate
                            let client_identity = {
                                let (_, session) = tls_stream.get_ref();
                                match session.peer_certificates().and_then(|certs| certs.first()) {
                                    Some(cert) => match extract_client_identity(cert.as_ref()) {
                                        Ok(id) => Some(id),
                                        Err(e) => {
                                            tracing::warn!(
                                                error = %e,
                                                "Failed to extract client identity from certificate"
                                            );
                                            return;
                                        }
                                    },
                                    None => None,
                                }
                            };

                            let _ = server.handle_stream(tls_stream, client_identity).await;
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Accept error in broker TLS listener");
                        break;
                    }
                }
            }
        });

        Ok((local_addr, handle))
    }

    async fn handle_connection(
        self: Arc<Self>,
        stream: TcpStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.handle_stream(stream, None).await
    }

    async fn handle_stream<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        self: Arc<Self>,
        stream: S,
        client_identity: Option<ClientIdentity>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut buf_reader = BufReader::new(reader);
        let max_frame = self.limits.max_frame_size;

        loop {
            let mut line = Vec::new();
            let mut take = (&mut buf_reader).take((max_frame + 1) as u64);
            let n = take.read_until(b'\n', &mut line).await?;
            if n == 0 {
                break;
            }

            if line.len() > max_frame {
                let err_resp = BrokerResponse::Error(format!(
                    "Frame size {} exceeds maximum allowed limit {}",
                    line.len(),
                    max_frame
                ));
                let mut serialized = serde_json::to_vec(&err_resp)?;
                serialized.push(b'\n');
                let _ = writer.write_all(&serialized).await;
                let _ = writer.flush().await;
                return Err(Box::new(BrokerError::FrameTooLarge {
                    size: line.len(),
                    max: max_frame,
                }));
            }

            let line_str = match std::str::from_utf8(&line) {
                Ok(s) => s.trim(),
                Err(e) => {
                    let err_resp = BrokerResponse::Error(format!("Malformed UTF-8: {}", e));
                    let mut serialized = serde_json::to_vec(&err_resp)?;
                    serialized.push(b'\n');
                    writer.write_all(&serialized).await?;
                    writer.flush().await?;
                    continue;
                }
            };

            if line_str.is_empty() {
                continue;
            }

            let req: BrokerRequest = match serde_json::from_str(line_str) {
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

            // Per-client identity & role enforcement (W09/W13):
            if let Some(ref identity) = client_identity {
                match &req {
                    BrokerRequest::Produce(_) if !identity.has_role("Producer") => {
                        let err_resp = BrokerResponse::Error(format!(
                            "Authorization denied: principal {:?} lacks Producer role (W09)",
                            identity.principal
                        ));
                        let mut serialized = serde_json::to_vec(&err_resp)?;
                        serialized.push(b'\n');
                        writer.write_all(&serialized).await?;
                        writer.flush().await?;
                        continue;
                    }
                    BrokerRequest::Fetch(_) if !identity.has_role("Consumer") => {
                        let err_resp = BrokerResponse::Error(format!(
                            "Authorization denied: principal {:?} lacks Consumer role (W09/W13)",
                            identity.principal
                        ));
                        let mut serialized = serde_json::to_vec(&err_resp)?;
                        serialized.push(b'\n');
                        writer.write_all(&serialized).await?;
                        writer.flush().await?;
                        continue;
                    }
                    BrokerRequest::Replicate(_) if !identity.has_role("Agent") => {
                        let err_resp = BrokerResponse::Error(format!(
                            "Authorization denied: principal {:?} lacks Agent role (W09)",
                            identity.principal
                        ));
                        let mut serialized = serde_json::to_vec(&err_resp)?;
                        serialized.push(b'\n');
                        writer.write_all(&serialized).await?;
                        writer.flush().await?;
                        continue;
                    }
                    _ => {}
                }
            }

            // Non-blocking Tokio worker thread dispatch with client identity (W09/W10/W13):
            let server = Arc::clone(&self);
            let id = client_identity.clone();
            let resp = match req {
                BrokerRequest::Produce(_)
                | BrokerRequest::Fetch(_)
                | BrokerRequest::Replicate(_) => tokio::task::spawn_blocking(move || {
                    server.dispatch_with_identity(req, id.as_ref())
                })
                .await
                .map_err(|e| std::io::Error::other(e.to_string()))?,
                _ => self.dispatch_with_identity(req, client_identity.as_ref()),
            };

            let mut serialized = serde_json::to_vec(&resp)?;
            serialized.push(b'\n');
            writer.write_all(&serialized).await?;
            writer.flush().await?;
        }
        Ok(())
    }
}

/// Lightweight async framed client connecting to a remote Kamish Broker daemon (plain TCP or TLS 1.3 mTLS).
pub struct BrokerClient {
    reader: Pin<Box<dyn AsyncBufRead + Send>>,
    writer: Pin<Box<dyn AsyncWrite + Send>>,
}

impl BrokerClient {
    /// Connect over plain TCP.
    pub async fn connect(addr: impl tokio::net::ToSocketAddrs) -> Result<Self, std::io::Error> {
        let stream = TcpStream::connect(addr).await?;
        let (reader, writer) = tokio::io::split(stream);
        let buf_reader = BufReader::new(reader);
        Ok(Self {
            reader: Box::pin(buf_reader),
            writer: Box::pin(writer),
        })
    }

    /// Connect over TLS 1.3 with mutual authentication (mTLS).
    pub async fn connect_tls(
        addr: impl tokio::net::ToSocketAddrs,
        domain: rustls::pki_types::ServerName<'static>,
        connector: tokio_rustls::TlsConnector,
    ) -> Result<Self, std::io::Error> {
        let tcp_stream = TcpStream::connect(addr).await?;
        let tls_stream = connector
            .connect(domain, tcp_stream)
            .await
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e))?;
        let (reader, writer) = tokio::io::split(tls_stream);
        let buf_reader = BufReader::new(reader);
        Ok(Self {
            reader: Box::pin(buf_reader),
            writer: Box::pin(writer),
        })
    }

    pub async fn send_request(
        &mut self,
        req: BrokerRequest,
    ) -> Result<BrokerResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut payload = serde_json::to_vec(&req)?;
        payload.push(b'\n');
        self.writer.write_all(&payload).await?;
        self.writer.flush().await?;

        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Connection closed by remote broker",
            )
            .into());
        }
        let resp: BrokerResponse = serde_json::from_str(line.trim())?;
        Ok(resp)
    }

    pub async fn produce(
        &mut self,
        req: ProduceRequest,
    ) -> Result<ProduceResponse, Box<dyn std::error::Error + Send + Sync>> {
        match self.send_request(BrokerRequest::Produce(req)).await? {
            BrokerResponse::Produce(p) => Ok(p),
            BrokerResponse::Error(e) => Err(std::io::Error::other(e).into()),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unexpected response: {:?}", other),
            )
            .into()),
        }
    }

    pub async fn fetch(
        &mut self,
        req: FetchRequest,
    ) -> Result<FetchResponse, Box<dyn std::error::Error + Send + Sync>> {
        match self.send_request(BrokerRequest::Fetch(req)).await? {
            BrokerResponse::Fetch(f) => Ok(f),
            BrokerResponse::Error(e) => Err(std::io::Error::other(e).into()),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unexpected response: {:?}", other),
            )
            .into()),
        }
    }

    pub async fn replicate(
        &mut self,
        req: ReplicationRequest,
    ) -> Result<ReplicationResponse, Box<dyn std::error::Error + Send + Sync>> {
        match self.send_request(BrokerRequest::Replicate(req)).await? {
            BrokerResponse::Replicate(r) => Ok(r),
            BrokerResponse::Error(e) => Err(std::io::Error::other(e).into()),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unexpected response: {:?}", other),
            )
            .into()),
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
            BrokerResponse::Error(e) => Err(std::io::Error::other(e).into()),
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

    pub async fn promote_partition(
        &mut self,
        topic: impl Into<String>,
        partition: u32,
        class_id: Option<String>,
        required_watermark: Option<u64>,
    ) -> Result<BrokerResponse, Box<dyn std::error::Error + Send + Sync>> {
        self.send_request(BrokerRequest::PromotePartition {
            topic: topic.into(),
            partition,
            class_id,
            required_watermark,
        })
        .await
    }

    pub async fn quarantine_divergent_suffix(
        &mut self,
        topic: impl Into<String>,
        partition: u32,
        agreed_watermark: u64,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        match self
            .send_request(BrokerRequest::QuarantineDivergentSuffix {
                topic: topic.into(),
                partition,
                agreed_watermark,
            })
            .await?
        {
            BrokerResponse::SuffixQuarantined { quarantined_count, .. } => Ok(quarantined_count),
            BrokerResponse::Error(e) => Err(std::io::Error::other(e).into()),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unexpected response: {:?}", other),
            )
            .into()),
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
    use kamish_meta::MetadataStore;
    use kamish_policy::{
        AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
    };
    use rand::rngs::OsRng;
    use std::collections::{HashMap, HashSet};

    fn setup_test_broker(
        local_region: &str,
        peers: Vec<String>,
    ) -> (BrokerServer, tempfile::TempDir) {
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

        classes.insert(
            "sealed-data".into(),
            ClassRule {
                class_id: "sealed-data".into(),
                description: "Requires unavailable sealed integration".into(),
                legal_regions: ["region-eu".into(), "region-us".into()].into(),
                destination_modes: [
                    ("region-eu".into(), DestinationMode::Cleartext),
                    ("region-us".into(), DestinationMode::SealedOnly),
                ]
                .into(),
                ack_mode: AckMode::LocalMajority,
                failover_candidates: vec![],
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
        let enforcer = Arc::new(Enforcer::new(
            local_region,
            Arc::new(RwLock::new(cache)),
            audit_log.clone(),
        ));
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
            residency_rule: Some(ResidencyPolicy::RestrictedReplication(vec![
                RegionId::UsEast,
            ])),
        };
        let res_restricted = broker_eu.produce(req_restricted).unwrap();
        assert!(res_restricted.sovereignty_allowed);

        // Egress hook permitted US_EAST: exactly 1 batch queued!
        assert_eq!(us_outbox.next_transfer_batch(10).len(), 1);
    }

    #[test]
    fn fetch_cannot_relabel_stored_records() {
        let (broker, _dir) = setup_test_broker("region-eu", vec![]);
        broker
            .produce(ProduceRequest {
                topic: "restricted".into(),
                partition: 0,
                producer_id: 1,
                producer_epoch: 1,
                sequence_base: 0,
                class_id: "eu-strict".into(),
                payload: b"private".to_vec(),
                residency_rule: None,
            })
            .unwrap();
        let error = broker
            .fetch(FetchRequest {
                topic: "restricted".into(),
                partition: 0,
                class_id: "global-data".into(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap_err();
        assert!(
            matches!(error, BrokerError::PolicyDenied(reason) if reason == "fetch_class_mismatch")
        );
    }

    #[test]
    fn simultaneous_retries_append_once_and_reject_changed_payload() {
        let (broker, _dir) = setup_test_broker("region-eu", vec![]);
        let broker = Arc::new(broker);
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let request = ProduceRequest {
            topic: "concurrent-retry".into(),
            partition: 0,
            producer_id: 1,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".into(),
            payload: vec![7; 100_000],
            residency_rule: None,
        };
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let broker = broker.clone();
                let barrier = barrier.clone();
                let request = request.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    broker.produce(request).unwrap()
                })
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap().base_offset, 0);
        }
        assert_eq!(
            broker
                .fetch(FetchRequest {
                    topic: request.topic.clone(),
                    partition: 0,
                    class_id: request.class_id.clone(),
                    start_offset: 0,
                    max_records: 100,
                })
                .unwrap()
                .records
                .len(),
            1
        );
        let mut changed = request;
        changed.payload = b"different".to_vec();
        assert!(broker.produce(changed).is_err());
    }

    #[test]
    fn topic_paths_cannot_escape_data_directory() {
        let (broker, dir) = setup_test_broker("region-eu", vec![]);
        for topic in ["../escape", "/absolute", "a/b", "a\\b", "", ".", ".."] {
            assert!(
                broker
                    .fetch(FetchRequest {
                        topic: topic.into(),
                        partition: 0,
                        class_id: "global-data".into(),
                        start_offset: 0,
                        max_records: 1,
                    })
                    .is_err(),
                "{topic}"
            );
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn startup_restores_sessions_and_preserves_narrowing_without_optional_hook() {
        let (mut broker, dir) = setup_test_broker("region-eu", vec!["region-us".into()]);
        broker.policy_engine = None;
        let request = ProduceRequest {
            topic: "restart".into(),
            partition: 0,
            producer_id: 42,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".into(),
            payload: b"local".to_vec(),
            residency_rule: Some(ResidencyPolicy::StrictLocal),
        };
        assert_eq!(broker.produce(request.clone()).unwrap().base_offset, 0);
        let enforcer = broker.enforcer.clone();
        drop(broker);
        let restarted = BrokerServer::try_new(
            "region-eu",
            enforcer,
            Arc::new(MetadataStore::new()),
            vec!["region-us".into()],
            dir.path(),
        )
        .unwrap();
        assert_eq!(restarted.produce(request.clone()).unwrap().base_offset, 0);
        assert!(
            restarted
                .get_outbox("region-us")
                .unwrap()
                .next_transfer_batch(10)
                .is_empty()
        );
        let mut changed = request;
        changed.residency_rule = Some(ResidencyPolicy::GlobalCleartext);
        assert!(restarted.produce(changed).is_err());
    }

    #[test]
    fn unsupported_sealed_replication_fails_before_storage_even_with_forged_flag() {
        let (source, _a) = setup_test_broker("region-eu", vec!["region-us".into()]);
        let (target, _b) = setup_test_broker("region-us", vec![]);
        let result = source.produce(ProduceRequest {
            topic: "sealed".into(),
            partition: 0,
            producer_id: 1,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "sealed-data".into(),
            payload: b"sensitive".to_vec(),
            residency_rule: None,
        });
        assert!(
            matches!(result, Err(BrokerError::PolicyDenied(reason)) if reason == "sealed_replication_not_configured")
        );
        assert!(
            source
                .get_outbox("region-us")
                .unwrap()
                .next_transfer_batch(10)
                .is_empty()
        );
        for is_sealed in [false, true] {
            let response = target
                .handle_replication(ReplicationRequest {
                    topic: "sealed".into(),
                    partition: 0,
                    source_region: "region-eu".into(),
                    target_region: "region-us".into(),
                    class_id: "sealed-data".into(),
                    epoch: 1,
                    from_offset: 0,
                    to_offset: 1,
                    payload: b"sensitive".to_vec(),
                    is_sealed,
                })
                .unwrap();
            assert!(!response.accepted);
        }
        assert_eq!(target.get_watermark("sealed", 0, "region-eu"), 0);
    }

    #[test]
    fn registered_wrong_or_expired_owner_cannot_acknowledge() {
        let (broker, _dir) = setup_test_broker("region-eu", vec![]);
        let req = ProduceRequest {
            topic: "owned".into(),
            partition: 0,
            producer_id: 1,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".into(),
            payload: vec![1],
            residency_rule: None,
        };
        let stream = kamish_meta::StreamId::new("owned", 0);
        broker
            .meta
            .compare_and_swap_epoch(&stream, 0, 7, "region-us", 60_000)
            .unwrap();
        assert!(broker.produce(req.clone()).is_err());
        // Transfer while region-us lease is active must fail with LeaseConflict
        let conflict_err = broker
            .meta
            .compare_and_swap_epoch(&stream, 7, 8, "region-eu", -1)
            .unwrap_err();
        assert!(matches!(
            conflict_err,
            kamish_meta::MetaError::LeaseConflict { .. }
        ));

        // After revoking the prior owner's lease, transfer succeeds
        broker.meta.revoke_lease(&stream, 0).unwrap();
        broker
            .meta
            .compare_and_swap_epoch(&stream, 7, 8, "region-eu", -1)
            .unwrap();
        assert!(broker.produce(req.clone()).is_err());
        broker
            .meta
            .compare_and_swap_epoch(&stream, 8, 9, "region-eu", 60_000)
            .unwrap();
        let response = broker.produce(req).unwrap();
        assert_eq!(response.committed_epoch, 9);
        assert_eq!(response.base_offset, 0);
    }

    #[test]
    fn replication_rejects_stale_conflicting_and_invalid_ranges_before_ack() {
        let (receiver, _dir) = setup_test_broker("region-us", vec![]);
        let request = ReplicationRequest {
            topic: "history".into(),
            partition: 0,
            source_region: "region-eu".into(),
            target_region: "region-us".into(),
            class_id: "global-data".into(),
            epoch: 7,
            from_offset: 0,
            to_offset: 1,
            payload: b"original".to_vec(),
            is_sealed: false,
        };
        assert!(
            receiver
                .handle_replication(request.clone())
                .unwrap()
                .accepted
        );
        assert!(
            receiver
                .handle_replication(request.clone())
                .unwrap()
                .accepted
        );
        let mut stale = request.clone();
        stale.epoch = 1;
        assert!(!receiver.handle_replication(stale).unwrap().accepted);
        let mut different = request.clone();
        different.payload = b"different".to_vec();
        assert!(!receiver.handle_replication(different).unwrap().accepted);
        let mut range = request;
        range.topic = "bad-range".into();
        range.to_offset = 5;
        assert!(!receiver.handle_replication(range).unwrap().accepted);
        assert_eq!(receiver.get_watermark("history", 0, "region-eu"), 1);
        assert_eq!(receiver.get_watermark("bad-range", 0, "region-eu"), 0);
    }

    #[test]
    fn changing_stream_policy_is_rejected_without_consuming_producer_sequence() {
        let (source, _dir) = setup_test_broker("region-eu", vec!["region-us".into()]);
        let request = ProduceRequest {
            topic: "fixed-policy".into(),
            partition: 0,
            producer_id: 42,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".into(),
            payload: vec![1],
            residency_rule: Some(ResidencyPolicy::StrictLocal),
        };
        assert!(source.produce(request.clone()).unwrap().sovereignty_allowed);
        let mut second = request;
        second.sequence_base = 1;
        second.residency_rule = Some(ResidencyPolicy::GlobalCleartext);
        assert!(!source.produce(second.clone()).unwrap().sovereignty_allowed);
        second.residency_rule = Some(ResidencyPolicy::StrictLocal);
        assert_eq!(source.produce(second).unwrap().base_offset, 1);
        assert!(
            source
                .get_outbox("region-us")
                .unwrap()
                .next_transfer_batch(10)
                .is_empty()
        );
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
            outbox_a
                .advance_watermark(rep_res.applied_watermark)
                .unwrap();
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
            outbox_a
                .advance_watermark(rep_res.applied_watermark)
                .unwrap();
        }

        assert_eq!(outbox_a.durable_watermark(), 5);
        assert_eq!(outbox_a.next_transfer_batch(10).len(), 0);

        let fetch_res = broker_b
            .fetch(FetchRequest {
                topic: "sensor-stream".to_string(),
                partition: 0,
                class_id: "global-data".to_string(),
                start_offset: 0,
                max_records: 10,
            })
            .unwrap();
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

        let prod_res = client
            .produce(ProduceRequest {
                topic: "tcp-stream".to_string(),
                partition: 0,
                producer_id: 777,
                producer_epoch: 1,
                sequence_base: 0,
                class_id: "global-data".to_string(),
                payload: b"tcp-payload".to_vec(),
                residency_rule: Some(ResidencyPolicy::GlobalCleartext),
            })
            .await
            .unwrap();
        assert!(prod_res.sovereignty_allowed);
        assert_eq!(prod_res.base_offset, 0);

        let fetch_res = client
            .fetch(FetchRequest {
                topic: "tcp-stream".to_string(),
                partition: 0,
                class_id: "global-data".to_string(),
                start_offset: 0,
                max_records: 5,
            })
            .await
            .unwrap();
        assert_eq!(fetch_res.records.len(), 1);
        assert_eq!(fetch_res.records[0].payload, b"tcp-payload");
    }

    #[tokio::test]
    async fn test_concurrent_multi_partition_lock_striping_and_reader_decoupling() {
        let (broker, _td) = setup_test_broker("region-eu", vec!["region-us".to_string()]);
        let broker_arc = Arc::new(broker);

        // Pre-populate 5 records in partition 0
        for seq in 0..5 {
            broker_arc
                .produce(ProduceRequest {
                    topic: "stripe-test".to_string(),
                    partition: 0,
                    producer_id: 1,
                    producer_epoch: 1,
                    sequence_base: seq,
                    class_id: "global-data".to_string(),
                    payload: format!("pre-{}", seq).into_bytes(),
                    residency_rule: Some(ResidencyPolicy::GlobalCleartext),
                })
                .unwrap();
        }

        // Spawn concurrent writers across 4 different partitions simultaneously
        let mut handles = Vec::new();
        for p in 0..4 {
            let b = Arc::clone(&broker_arc);
            let h = tokio::task::spawn_blocking(move || {
                for seq in 0..10 {
                    let req = ProduceRequest {
                        topic: "stripe-test".to_string(),
                        partition: p,
                        producer_id: 10 + p as u64,
                        producer_epoch: 1,
                        sequence_base: seq,
                        class_id: "global-data".to_string(),
                        payload: format!("payload-p{}-seq{}", p, seq).into_bytes(),
                        residency_rule: Some(ResidencyPolicy::GlobalCleartext),
                    };
                    let res = b.produce(req).unwrap();
                    assert!(res.sovereignty_allowed);
                }
            });
            handles.push(h);
        }

        // Concurrently, readers fetch from partition 0 while writes are ongoing
        let reader_broker = Arc::clone(&broker_arc);
        let reader_handle = tokio::spawn(async move {
            for _ in 0..10 {
                let f = reader_broker
                    .fetch(FetchRequest {
                        topic: "stripe-test".to_string(),
                        partition: 0,
                        class_id: "global-data".to_string(),
                        start_offset: 0,
                        max_records: 5,
                    })
                    .unwrap();
                assert!(f.records.len() >= 5);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        });

        for h in handles {
            h.await.unwrap();
        }
        reader_handle.await.unwrap();

        // Verify partition 0 has 5 pre + 10 writes = 15 records
        let f0 = broker_arc
            .fetch(FetchRequest {
                topic: "stripe-test".to_string(),
                partition: 0,
                class_id: "global-data".to_string(),
                start_offset: 0,
                max_records: 20,
            })
            .unwrap();
        assert_eq!(f0.records.len(), 15);

        // Verify partitions 1, 2, 3 have 10 records each
        for p in 1..4 {
            let fp = broker_arc
                .fetch(FetchRequest {
                    topic: "stripe-test".to_string(),
                    partition: p,
                    class_id: "global-data".to_string(),
                    start_offset: 0,
                    max_records: 20,
                })
                .unwrap();
            assert_eq!(fp.records.len(), 10);
            for (idx, rec) in fp.records.iter().enumerate() {
                assert_eq!(rec.base_offset, idx as u64);
            }
        }
    }

    #[test]
    fn test_broker_claim_partition_ownership_and_epoch_fencing() {
        let meta = Arc::new(MetadataStore::new_multi_region());
        let (mut broker_a, _dir_a) = setup_test_broker("region-eu", vec![]);
        broker_a.meta = meta.clone();

        let (mut broker_b, _dir_b) = setup_test_broker("region-us", vec![]);
        broker_b.meta = meta.clone();

        let req = ProduceRequest {
            topic: "claims".into(),
            partition: 0,
            producer_id: 1,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-data".into(),
            payload: vec![1, 2, 3],
            residency_rule: None,
        };

        // 1. Unregistered partition in MultiRegion mode fails closed
        let unreg_err = broker_a.produce(req.clone()).unwrap_err();
        assert!(
            matches!(unreg_err, BrokerError::PolicyDenied(ref s) if s == "unregistered_stream_in_multi_region_mode")
        );

        // 2. Broker A claims epoch 1
        let claim_a = broker_a
            .claim_partition_ownership("claims", 0, 0, 1, 60_000)
            .unwrap();
        assert_eq!(claim_a.epoch, 1);
        assert_eq!(claim_a.owner_region, "region-eu");

        // 3. Broker B attempts to claim epoch 1 with expected_epoch 0 -> fails with EpochFenced
        let claim_b = broker_b
            .claim_partition_ownership("claims", 0, 0, 1, 60_000)
            .unwrap_err();
        assert!(matches!(
            claim_b,
            BrokerError::EpochFenced {
                current: 1,
                attempted: 0
            }
        ));

        // 4. Broker A produces successfully at epoch 1
        let resp_a = broker_a.produce(req.clone()).unwrap();
        assert_eq!(resp_a.committed_epoch, 1);
        assert!(resp_a.sovereignty_allowed);

        // 5. Broker B attempts to produce -> fails with not_current_partition_owner
        let err_b = broker_b.produce(req).unwrap_err();
        assert!(
            matches!(err_b, BrokerError::PolicyDenied(ref s) if s == "not_current_partition_owner")
        );
    }
}
