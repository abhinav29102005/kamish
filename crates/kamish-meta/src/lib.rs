//! Metadata, Policy Engine, Regional Topology, and Observability for Kamish.
//!
//! Complies with `project.md` and Developer B Specification:
//! - Canonical StreamEvent and Trait Contract (§3.A, §3.B)
//! - RegionId Topology (US_EAST, EU_WEST, AP_SOUTH)
//! - Concrete PolicyEngine with Auditable Security Rejection Logging
//! - Replication Lag Metrics & Synthetic Chaos Latency Injector

use chrono::Utc;
use kamish_audit::{AuditAction, AuditLog, DecisionOutcome};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

pub mod etcd;
pub use etcd::EtcdMetadataAuthority;
use std::time::Duration;
use thiserror::Error;

// ============================================================================
// 1. Shared Contract: Region Topology & Canonical Stream Event (§3.A)
// ============================================================================

/// Strongly-typed region identifier for multi-region topology (§1.2, §2.2).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegionId {
    #[serde(rename = "US_EAST")]
    UsEast,
    #[serde(rename = "EU_WEST")]
    EuWest,
    #[serde(rename = "AP_SOUTH")]
    ApSouth,
    #[serde(untagged)]
    Custom(String),
}

impl fmt::Display for RegionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegionId::UsEast => write!(f, "US_EAST"),
            RegionId::EuWest => write!(f, "EU_WEST"),
            RegionId::ApSouth => write!(f, "AP_SOUTH"),
            RegionId::Custom(s) => write!(f, "{}", s),
        }
    }
}

/// Sovereign data residency policy rules (§1.4, §2.4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResidencyPolicy {
    /// Data must remain strictly within the local ingestion region.
    StrictLocal,
    /// Data may replicate only to explicitly enumerated target regions.
    RestrictedReplication(Vec<RegionId>),
    /// Open data class: unrestricted cross-region replication.
    GlobalCleartext,
    /// Ciphertext-only replication: payload must be encrypted; keys stay in home region.
    SealedCiphertext(Vec<RegionId>),
}

/// Canonical Stream Event structure shared across all broker and replication workers (§3.A).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamEvent {
    pub id: u64,
    pub topic: String,
    pub partition_key: String,
    pub origin_region: RegionId,
    pub residency_rule: ResidencyPolicy,
    pub timestamp_ms: u64,
    pub payload: Vec<u8>,
}

impl StreamEvent {
    pub fn new(
        id: u64,
        topic: impl Into<String>,
        partition_key: impl Into<String>,
        origin_region: RegionId,
        residency_rule: ResidencyPolicy,
        payload: Vec<u8>,
    ) -> Self {
        let now_ms = Utc::now().timestamp_millis() as u64;
        Self {
            id,
            topic: topic.into(),
            partition_key: partition_key.into(),
            origin_region,
            residency_rule,
            timestamp_ms: now_ms,
            payload,
        }
    }
}

// ============================================================================
// 2. Policy Enforcement Trait & Concrete Engine (§3.B, Developer B Deliverable)
// ============================================================================

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum PolicyViolation {
    #[error("Ingestion prohibited in region {local_region}: origin is {origin_region} ({reason})")]
    IngestionDenied {
        origin_region: RegionId,
        local_region: RegionId,
        reason: String,
    },
    #[error(
        "Replication from {source_region} to {target_region} prohibited by policy {rule:?} ({reason})"
    )]
    ReplicationDenied {
        source_region: RegionId,
        target_region: RegionId,
        rule: ResidencyPolicy,
        reason: String,
    },
}

/// Trait contract maintained by Developer B, consumed by Developer A storage & replication engine (§3.B).
pub trait PolicyEngine: Send + Sync {
    /// Validates whether an incoming event may be ingested in the local region.
    fn validate_ingestion(
        &self,
        event: &StreamEvent,
        local_region: &RegionId,
    ) -> Result<(), PolicyViolation>;

    /// Validates whether an event may be replicated across regions.
    fn validate_replication(
        &self,
        event: &StreamEvent,
        source_region: &RegionId,
        target_region: &RegionId,
    ) -> Result<(), PolicyViolation>;
}

/// Concrete implementation of the PolicyEngine integrating zero-payload hash-chained audit logging.
#[derive(Clone)]
pub struct KamishPolicyEngine {
    audit_log: AuditLog,
}

impl KamishPolicyEngine {
    pub fn new(audit_log: AuditLog) -> Self {
        Self { audit_log }
    }

    pub fn audit_log(&self) -> &AuditLog {
        &self.audit_log
    }
}

impl PolicyEngine for KamishPolicyEngine {
    fn validate_ingestion(
        &self,
        event: &StreamEvent,
        local_region: &RegionId,
    ) -> Result<(), PolicyViolation> {
        // Enforce that origin region matches local region for StrictLocal
        if event.residency_rule == ResidencyPolicy::StrictLocal
            && event.origin_region != *local_region
        {
            let reason = format!(
                "StrictLocal event origin {} does not match local ingestion region {}",
                event.origin_region, local_region
            );
            let _ = self.audit_log.record(
                AuditAction::Ingress,
                &event.topic,
                event.origin_region.to_string(),
                local_region.to_string(),
                1,
                DecisionOutcome::Deny,
                &reason,
            );
            return Err(PolicyViolation::IngestionDenied {
                origin_region: event.origin_region.clone(),
                local_region: local_region.clone(),
                reason,
            });
        }

        let _ = self.audit_log.record(
            AuditAction::Ingress,
            &event.topic,
            event.origin_region.to_string(),
            local_region.to_string(),
            1,
            DecisionOutcome::AllowCleartext,
            "ok_ingest",
        );
        Ok(())
    }

    fn validate_replication(
        &self,
        event: &StreamEvent,
        source_region: &RegionId,
        target_region: &RegionId,
    ) -> Result<(), PolicyViolation> {
        // Source and target are identical -> local reflection is not a remote egress
        if source_region == target_region {
            return Ok(());
        }

        match &event.residency_rule {
            ResidencyPolicy::StrictLocal => {
                let reason = format!(
                    "StrictLocal policy forbids egress replication to remote region {}",
                    target_region
                );
                // Auditable security rejection log (§5 Developer B Checklist 2)
                let _ = self.audit_log.record(
                    AuditAction::ReplicationSender,
                    &event.topic,
                    source_region.to_string(),
                    target_region.to_string(),
                    1,
                    DecisionOutcome::Deny,
                    &reason,
                );
                Err(PolicyViolation::ReplicationDenied {
                    source_region: source_region.clone(),
                    target_region: target_region.clone(),
                    rule: ResidencyPolicy::StrictLocal,
                    reason,
                })
            }

            ResidencyPolicy::RestrictedReplication(allowed_regions) => {
                if !allowed_regions.contains(target_region) {
                    let reason = format!(
                        "Target region {} not in allowed legal set {:?}",
                        target_region, allowed_regions
                    );
                    let _ = self.audit_log.record(
                        AuditAction::ReplicationSender,
                        &event.topic,
                        source_region.to_string(),
                        target_region.to_string(),
                        1,
                        DecisionOutcome::Deny,
                        &reason,
                    );
                    return Err(PolicyViolation::ReplicationDenied {
                        source_region: source_region.clone(),
                        target_region: target_region.clone(),
                        rule: event.residency_rule.clone(),
                        reason,
                    });
                }

                let _ = self.audit_log.record(
                    AuditAction::ReplicationSender,
                    &event.topic,
                    source_region.to_string(),
                    target_region.to_string(),
                    1,
                    DecisionOutcome::AllowCleartext,
                    "ok_restricted_replication",
                );
                Ok(())
            }

            ResidencyPolicy::GlobalCleartext => {
                let _ = self.audit_log.record(
                    AuditAction::ReplicationSender,
                    &event.topic,
                    source_region.to_string(),
                    target_region.to_string(),
                    1,
                    DecisionOutcome::AllowCleartext,
                    "ok_global_cleartext",
                );
                Ok(())
            }

            ResidencyPolicy::SealedCiphertext(allowed_regions) => {
                if !allowed_regions.contains(target_region) {
                    let reason = format!(
                        "Target region {} not permitted for sealed copy",
                        target_region
                    );
                    let _ = self.audit_log.record(
                        AuditAction::ReplicationSender,
                        &event.topic,
                        source_region.to_string(),
                        target_region.to_string(),
                        1,
                        DecisionOutcome::Deny,
                        &reason,
                    );
                    return Err(PolicyViolation::ReplicationDenied {
                        source_region: source_region.clone(),
                        target_region: target_region.clone(),
                        rule: event.residency_rule.clone(),
                        reason,
                    });
                }

                let _ = self.audit_log.record(
                    AuditAction::ReplicationSender,
                    &event.topic,
                    source_region.to_string(),
                    target_region.to_string(),
                    1,
                    DecisionOutcome::AllowSealed,
                    "ok_sealed_ciphertext",
                );
                Ok(())
            }
        }
    }
}

// ============================================================================
// 3. Observability, Replication Lag & Synthetic Chaos Injector (§5 B.3, B.4)
// ============================================================================

/// Real-time metrics tracker for replication lag, throughput, and link RTT (§5 B.3).
#[derive(Clone, Default)]
pub struct ReplicationMetricsTracker {
    inner: Arc<RwLock<MetricsInner>>,
}

#[derive(Default)]
struct MetricsInner {
    local_committed_offset: HashMap<(String, u32), u64>,
    remote_acked_offset: HashMap<(String, u32, RegionId), u64>,
    link_rtt_ms: HashMap<RegionId, f64>,
}

impl ReplicationMetricsTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_local_committed(&self, topic: &str, partition: u32, offset: u64) {
        let mut inner = self.inner.write().unwrap();
        inner
            .local_committed_offset
            .insert((topic.to_string(), partition), offset);
    }

    pub fn set_remote_acked(&self, topic: &str, partition: u32, region: RegionId, offset: u64) {
        let mut inner = self.inner.write().unwrap();
        inner
            .remote_acked_offset
            .insert((topic.to_string(), partition, region), offset);
    }

    pub fn set_link_rtt(&self, region: RegionId, rtt_ms: f64) {
        let mut inner = self.inner.write().unwrap();
        inner.link_rtt_ms.insert(region, rtt_ms);
    }

    /// Exposes formula: ReplicationLag = LocalCommittedOffset - RemoteAckedOffset (§5 B.3).
    pub fn get_replication_lag(&self, topic: &str, partition: u32, region: &RegionId) -> u64 {
        let inner = self.inner.read().unwrap();
        let local = inner
            .local_committed_offset
            .get(&(topic.to_string(), partition))
            .copied()
            .unwrap_or(0);
        let remote = inner
            .remote_acked_offset
            .get(&(topic.to_string(), partition, region.clone()))
            .copied()
            .unwrap_or(0);
        local.saturating_sub(remote)
    }

    pub fn get_link_rtt(&self, region: &RegionId) -> f64 {
        let inner = self.inner.read().unwrap();
        inner.link_rtt_ms.get(region).copied().unwrap_or(0.0)
    }
}

/// Chaos Proxy / Synthetic WAN Delay Injector (§5 B.4, Gate 3).
#[derive(Clone)]
pub struct SyntheticChaosInjector {
    delay_ms: Arc<RwLock<u64>>,
    _loss_rate: Arc<RwLock<f64>>,
    partitioned: Arc<RwLock<bool>>,
}

impl Default for SyntheticChaosInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntheticChaosInjector {
    pub fn new() -> Self {
        Self {
            delay_ms: Arc::new(RwLock::new(0)),
            _loss_rate: Arc::new(RwLock::new(0.0)),
            partitioned: Arc::new(RwLock::new(false)),
        }
    }

    pub fn set_delay(&self, delay_ms: u64) {
        let mut d = self.delay_ms.write().unwrap();
        *d = delay_ms;
    }

    pub fn set_partition(&self, is_partitioned: bool) {
        let mut p = self.partitioned.write().unwrap();
        *p = is_partitioned;
    }

    /// Applies simulated link conditions non-blockingly using Tokio timer.
    pub async fn simulate_link_traversal(&self) -> Result<(), &'static str> {
        if *self.partitioned.read().unwrap() {
            return Err("NETWORK_PARTITION: Link severed");
        }

        let delay = *self.delay_ms.read().unwrap();
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }

        Ok(())
    }
}

// ============================================================================
// 4. Metadata Entity-Relationship Store (§2.5) & MetadataAuthority Trait
// ============================================================================

#[derive(Debug, Error)]
pub enum MetaError {
    #[error("Topic '{0}' not found")]
    TopicNotFound(String),

    #[error("Partition {partition} for topic '{topic}' not found")]
    PartitionNotFound { topic: String, partition: u32 },

    #[error("Stale epoch update rejected: current epoch is {current}, proposed was {proposed}")]
    StaleEpoch { current: u64, proposed: u64 },

    #[error("Ownership lease conflict for partition {partition}")]
    LeaseConflict { partition: u32 },

    #[error("Lease not found or expired: id {0}")]
    LeaseNotFound(u64),

    #[error("Compare-and-swap conflict: expected epoch {expected}, actual epoch {actual}")]
    CasConflict { expected: u64, actual: u64 },

    #[error("Unregistered stream in multi-region mode: {topic}/{partition}")]
    UnregisteredStream { topic: String, partition: u32 },

    #[error("Policy floor regression rejected: current floor {current}, proposed {proposed}")]
    PolicyFloorRegression { current: u64, proposed: u64 },

    #[error("Authority error or backend communication failure: {0}")]
    AuthorityError(String),
}

/// Partition ownership with monotonic epoch fencing token (§2.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionOwnership {
    pub topic: String,
    pub partition: u32,
    pub owner_region: String,
    pub epoch: u64,
    pub lease_expiry_ns: i64,
}

impl PartitionOwnership {
    pub fn is_lease_valid(&self, now_ns: i64) -> bool {
        now_ns < self.lease_expiry_ns
    }
}

/// Stream identification tuple (topic and partition).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamId {
    pub topic: String,
    pub partition: u32,
}

impl StreamId {
    pub fn new(topic: impl Into<String>, partition: u32) -> Self {
        Self {
            topic: topic.into(),
            partition,
        }
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.topic, self.partition)
    }
}

/// Operational deployment mode of the broker metadata authority (§2.1, CONTRACT.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentMode {
    /// Explicit single-node compatibility mode (isolated development, local authority fallback).
    SingleNodeCompatibility,
    /// Multi-region distributed mode (consensus quorum required, unknown streams fail closed).
    MultiRegion,
}

/// Lease grant issued by metadata authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseGrant {
    pub lease_id: u64,
    pub stream: StreamId,
    pub holder_region: String,
    pub granted_at_ns: i64,
    pub expiry_ns: i64,
    pub epoch: u64,
}

impl LeaseGrant {
    pub fn is_valid(&self, now_ns: i64) -> bool {
        now_ns < self.expiry_ns
    }
}

/// Acknowledgment of a lease revocation with effective boundary timestamp.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationAck {
    pub lease_id: u64,
    pub stream: StreamId,
    pub revoked_at_ns: i64,
    pub effective_boundary_ns: i64,
}

/// Quorum-issued source identity and generation (§3.B, CONTRACT.md).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentityGrant {
    pub cluster_id: String,
    pub node_id: String,
    pub generation: u64,
    pub issued_at_ns: i64,
}

/// Signed audit checkpoint committed to the metadata witness quorum (§3.C, CONTRACT.md, W08).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditCheckpoint {
    pub stream: StreamId,
    pub sequence: u64,
    pub tail_hash: String,
    pub record_count: u64,
    pub signature_hex: String,
    pub witness_timestamp_ns: i64,
}

/// Receipt confirming that an audit checkpoint was durably committed to the witness quorum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointReceipt {
    pub stream: StreamId,
    pub checkpoint_sequence: u64,
    pub committed_revision: u64,
    pub anchored_at_ns: i64,
}

/// Remote replica state with durable applied watermark (§2.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaState {
    pub topic: String,
    pub partition: u32,
    pub region_id: String,
    pub dest_mode: String,
    pub durable_watermark: u64,
}

/// Outbox stream metadata (§2.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxStreamMeta {
    pub topic: String,
    pub partition: u32,
    pub dest_region: String,
    pub epoch: u64,
    pub next_offset: u64,
    pub bytes_queued: u64,
}

/// Topic descriptor entity (§2.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicMeta {
    pub topic_id: String,
    pub class_id: String,
    pub partition_count: u32,
}

/// Authoritative metadata trait for partition ownership, leases, policy versions,
/// source identity, and audit checkpoints.
///
/// Complies with:
/// - Master Plan Phase 1 (Authority)
/// - `docs/CONTRACT.md` (§2.1, §2.2, §3.B, §3.C)
/// - NFR-01: Metadata quorum is NOT consulted on the local write path.
/// - The broker core depends ONLY on this trait abstraction.
pub trait MetadataAuthority: kamish_audit::AuditWitnessAuthority + Send + Sync {
    /// Deployment mode of this metadata authority (SingleNodeCompatibility or MultiRegion).
    fn deployment_mode(&self) -> DeploymentMode;

    /// Compare-and-swap epoch operation across the metadata authority.
    /// Commits a bumped epoch and new owner region if the current committed epoch matches `expected_epoch`.
    fn compare_and_swap_epoch(
        &self,
        stream: &StreamId,
        expected_epoch: u64,
        new_epoch: u64,
        new_owner_region: &str,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError>;

    /// Grants a time-limited lease to `holder_region` for the specified `stream`.
    fn grant_lease(
        &self,
        stream: &StreamId,
        holder_region: &str,
        duration_ms: i64,
    ) -> Result<LeaseGrant, MetaError>;

    /// Renews an active lease if not revoked or expired.
    fn renew_lease(
        &self,
        stream: &StreamId,
        lease_id: u64,
        duration_ms: i64,
    ) -> Result<LeaseGrant, MetaError>;

    /// Revokes an active lease with a barrier bound.
    fn revoke_lease(&self, stream: &StreamId, lease_id: u64) -> Result<RevocationAck, MetaError>;

    /// Returns the authoritative policy-version floor for the cluster.
    fn get_policy_floor(&self, cluster_id: &str) -> Result<u64, MetaError>;

    /// Authoritatively advances the cluster's durable policy-version floor.
    fn advance_policy_floor(&self, cluster_id: &str, new_floor: u64) -> Result<u64, MetaError>;

    /// Issues an authoritative source identity and monotonic generation from the quorum.
    fn issue_source_identity(
        &self,
        cluster_id: &str,
        node_id: &str,
    ) -> Result<SourceIdentityGrant, MetaError>;

    /// Commits a signed audit checkpoint `(tail_hash, record_count)` to the witness quorum.
    fn commit_audit_checkpoint(
        &self,
        checkpoint: AuditCheckpoint,
    ) -> Result<CheckpointReceipt, MetaError>;

    /// Reads the latest committed audit checkpoint for a stream from the witness quorum.
    fn read_audit_checkpoint(
        &self,
        stream: &StreamId,
    ) -> Result<Option<AuditCheckpoint>, MetaError>;

    /// Returns the locally verified/cached partition ownership without consulting quorum per write (NFR-01).
    fn get_ownership(&self, topic: &str, partition: u32) -> Option<PartitionOwnership>;

    /// Reacquires or renews local write authority for a persisted stream belonging to `region`.
    fn reacquire_ownership(
        &self,
        stream: &StreamId,
        region: &str,
        duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError> {
        let _ = (stream, region, duration_ms);
        Err(MetaError::AuthorityError(
            "reacquire_ownership not supported by this authority".into(),
        ))
    }
}

impl kamish_audit::AuditWitnessAuthority for MetadataStore {
    #[allow(clippy::too_many_arguments)]
    fn commit_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
        sequence: u64,
        tail_hash: &str,
        record_count: u64,
        signature_hex: &str,
        timestamp_ns: i64,
    ) -> Result<(), kamish_audit::AuditError> {
        let cp = AuditCheckpoint {
            stream: StreamId::new(stream_topic, stream_partition),
            sequence,
            tail_hash: tail_hash.to_string(),
            record_count,
            signature_hex: signature_hex.to_string(),
            witness_timestamp_ns: timestamp_ns,
        };
        self.commit_audit_checkpoint(cp)
            .map(|_| ())
            .map_err(|e| kamish_audit::AuditError::WitnessError(e.to_string()))
    }

    fn read_latest_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
    ) -> Result<Option<kamish_audit::AuditWitnessCheckpointData>, kamish_audit::AuditError> {
        let stream = StreamId::new(stream_topic, stream_partition);
        let cp = self
            .read_audit_checkpoint(&stream)
            .map_err(|e| kamish_audit::AuditError::WitnessError(e.to_string()))?;
        Ok(cp.map(|c| kamish_audit::AuditWitnessCheckpointData {
            sequence: c.sequence,
            tail_hash: c.tail_hash,
            record_count: c.record_count,
            signature_hex: c.signature_hex,
            timestamp_ns: c.witness_timestamp_ns,
        }))
    }
}

impl kamish_audit::AuditWitnessAuthority for EtcdMetadataAuthority {
    #[allow(clippy::too_many_arguments)]
    fn commit_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
        sequence: u64,
        tail_hash: &str,
        record_count: u64,
        signature_hex: &str,
        timestamp_ns: i64,
    ) -> Result<(), kamish_audit::AuditError> {
        let cp = AuditCheckpoint {
            stream: StreamId::new(stream_topic, stream_partition),
            sequence,
            tail_hash: tail_hash.to_string(),
            record_count,
            signature_hex: signature_hex.to_string(),
            witness_timestamp_ns: timestamp_ns,
        };
        self.commit_audit_checkpoint(cp)
            .map(|_| ())
            .map_err(|e| kamish_audit::AuditError::WitnessError(e.to_string()))
    }

    fn read_latest_checkpoint(
        &self,
        stream_topic: &str,
        stream_partition: u32,
    ) -> Result<Option<kamish_audit::AuditWitnessCheckpointData>, kamish_audit::AuditError> {
        let stream = StreamId::new(stream_topic, stream_partition);
        let cp = self
            .read_audit_checkpoint(&stream)
            .map_err(|e| kamish_audit::AuditError::WitnessError(e.to_string()))?;
        Ok(cp.map(|c| kamish_audit::AuditWitnessCheckpointData {
            sequence: c.sequence,
            tail_hash: c.tail_hash,
            record_count: c.record_count,
            signature_hex: c.signature_hex,
            timestamp_ns: c.witness_timestamp_ns,
        }))
    }
}

/// Local state tracking for replication watermarks and outbox streams.
/// This state is strictly local to the node/region and does NOT go through
/// the metadata consensus quorum on the replication path.
pub trait LocalReplicationTracker: Send + Sync {
    /// Updates the durable replication watermark for an ingress/egress peer.
    fn update_watermark(&self, topic: &str, partition: u32, region: &str, watermark: u64);

    /// Returns the recorded replication watermark for a region.
    fn get_watermark(&self, topic: &str, partition: u32, region: &str) -> u64;

    /// Records outbox stream tracking metadata.
    fn record_outbox_stream(&self, meta: OutboxStreamMeta);

    /// Retrieves outbox stream tracking metadata.
    fn get_outbox_stream(
        &self,
        topic: &str,
        partition: u32,
        dest: &str,
    ) -> Option<OutboxStreamMeta>;
}

// ============================================================================
// 4.1 Monotonic Clock & Local Lease Validation (CONTRACT §2.1, Phase 1)
// ============================================================================

/// Injectable monotonic clock interface for lease duration tracking.
pub trait MonotonicClock: Send + Sync {
    /// Returns current monotonic milliseconds. Guaranteed never to regress.
    fn now_monotonic_ms(&self) -> u64;
}

/// Standard system monotonic clock backed by an initialized `Instant`.
#[derive(Clone)]
pub struct SystemMonotonicClock {
    start: std::time::Instant,
}

impl Default for SystemMonotonicClock {
    fn default() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl SystemMonotonicClock {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MonotonicClock for SystemMonotonicClock {
    fn now_monotonic_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

/// Mock monotonic clock for deterministic unit tests covering pause, skew, and expiry.
pub struct MockMonotonicClock {
    now_ms: std::sync::atomic::AtomicU64,
}

impl MockMonotonicClock {
    pub fn new(initial_ms: u64) -> Self {
        Self {
            now_ms: std::sync::atomic::AtomicU64::new(initial_ms),
        }
    }

    pub fn advance_ms(&self, delta_ms: u64) {
        self.now_ms
            .fetch_add(delta_ms, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn set_ms(&self, ms: u64) {
        self.now_ms.store(ms, std::sync::atomic::Ordering::SeqCst);
    }
}

impl MonotonicClock for MockMonotonicClock {
    fn now_monotonic_ms(&self) -> u64 {
        self.now_ms.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Locally verified write lease tracked via a monotonic clock.
///
/// Complies with `docs/CONTRACT.md` (§2.1, §2.3) and Master Plan Phase 1:
/// - Lease validity is computed from a local monotonic clock measured from when
///   the grant was received.
/// - Never computed from etcd or wall-clock timestamps.
/// - Incorporates a safety allowance for max clock drift and transit delay.
#[derive(Clone, Debug)]
pub struct LocalLease {
    pub lease_id: u64,
    pub stream: StreamId,
    pub holder_region: String,
    pub epoch: u64,
    pub received_monotonic_ms: u64,
    pub effective_duration_ms: u64,
}

impl LocalLease {
    pub fn from_grant(
        grant: &LeaseGrant,
        received_monotonic_ms: u64,
        max_clock_drift_ms: u64,
        transit_delay_allowance_ms: u64,
    ) -> Self {
        let total_allowance = max_clock_drift_ms + transit_delay_allowance_ms;
        let grant_duration_ms = ((grant.expiry_ns - grant.granted_at_ns) / 1_000_000).max(0) as u64;
        let effective_duration_ms = grant_duration_ms.saturating_sub(total_allowance);

        Self {
            lease_id: grant.lease_id,
            stream: grant.stream.clone(),
            holder_region: grant.holder_region.clone(),
            epoch: grant.epoch,
            received_monotonic_ms,
            effective_duration_ms,
        }
    }

    pub fn is_valid(&self, current_monotonic_ms: u64) -> bool {
        if current_monotonic_ms < self.received_monotonic_ms {
            return false;
        }
        let elapsed = current_monotonic_ms - self.received_monotonic_ms;
        elapsed < self.effective_duration_ms
    }

    pub fn remaining_ms(&self, current_monotonic_ms: u64) -> u64 {
        if !self.is_valid(current_monotonic_ms) {
            return 0;
        }
        self.effective_duration_ms
            .saturating_sub(current_monotonic_ms - self.received_monotonic_ms)
    }

    /// Converts this locally verified lease to a `PartitionOwnership` descriptor
    /// with `lease_expiry_ns` calculated relative to remaining monotonic time.
    pub fn to_partition_ownership(&self, current_monotonic_ms: u64) -> Option<PartitionOwnership> {
        if !self.is_valid(current_monotonic_ms) {
            return None;
        }
        let remaining_ms = self.remaining_ms(current_monotonic_ms);
        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let lease_expiry_ns = now_ns + (remaining_ms as i64 * 1_000_000);
        Some(PartitionOwnership {
            topic: self.stream.topic.clone(),
            partition: self.stream.partition,
            owner_region: self.holder_region.clone(),
            epoch: self.epoch,
            lease_expiry_ns,
        })
    }
}

/// Central in-memory, thread-safe Metadata Store (D4 in DFDs §2.7.2).
#[derive(Clone)]
pub struct MetadataStore {
    inner: Arc<RwLock<MetadataStoreInner>>,
}

struct MetadataStoreInner {
    mode: DeploymentMode,
    topics: HashMap<String, TopicMeta>,
    ownerships: HashMap<(String, u32), PartitionOwnership>,
    leases: HashMap<u64, LeaseGrant>,
    next_lease_id: u64,
    policy_floors: HashMap<String, u64>,
    source_generations: HashMap<String, u64>,
    audit_checkpoints: HashMap<StreamId, Vec<AuditCheckpoint>>,
    replicas: HashMap<(String, u32, String), ReplicaState>,
    outbox_streams: HashMap<(String, u32, String), OutboxStreamMeta>,
}

impl Default for MetadataStoreInner {
    fn default() -> Self {
        Self {
            mode: DeploymentMode::SingleNodeCompatibility,
            topics: HashMap::new(),
            ownerships: HashMap::new(),
            leases: HashMap::new(),
            next_lease_id: 1,
            policy_floors: HashMap::new(),
            source_generations: HashMap::new(),
            audit_checkpoints: HashMap::new(),
            replicas: HashMap::new(),
            outbox_streams: HashMap::new(),
        }
    }
}

impl Default for MetadataStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MetadataStoreInner::default())),
        }
    }
}

impl MetadataStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_mode(mode: DeploymentMode) -> Self {
        Self {
            inner: Arc::new(RwLock::new(MetadataStoreInner {
                mode,
                ..Default::default()
            })),
        }
    }

    pub fn new_multi_region() -> Self {
        Self::new_with_mode(DeploymentMode::MultiRegion)
    }

    pub fn set_deployment_mode(&self, mode: DeploymentMode) {
        let mut inner = self.inner.write().unwrap();
        inner.mode = mode;
    }

    pub fn create_topic(
        &self,
        topic_id: impl Into<String>,
        class_id: impl Into<String>,
        partition_count: u32,
    ) -> TopicMeta {
        let topic_id = topic_id.into();
        let class_id = class_id.into();
        let topic = TopicMeta {
            topic_id: topic_id.clone(),
            class_id,
            partition_count,
        };

        let mut inner = self.inner.write().unwrap();
        inner.topics.insert(topic_id, topic.clone());
        topic
    }

    pub fn set_ownership(
        &self,
        topic: &str,
        partition: u32,
        owner_region: impl Into<String>,
        epoch: u64,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError> {
        let mut inner = self.inner.write().unwrap();
        if inner.mode == DeploymentMode::MultiRegion {
            return Err(MetaError::AuthorityError(
                "MultiRegion mode cannot set ownership except through compare_and_swap_epoch"
                    .to_string(),
            ));
        }
        let key = (topic.to_string(), partition);

        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let lease_expiry_ns = now_ns + (lease_duration_ms * 1_000_000);

        #[allow(clippy::collapsible_if)]
        if let Some(existing) = inner.ownerships.get(&key) {
            if epoch < existing.epoch {
                return Err(MetaError::StaleEpoch {
                    current: existing.epoch,
                    proposed: epoch,
                });
            }
        }

        let ownership = PartitionOwnership {
            topic: topic.to_string(),
            partition,
            owner_region: owner_region.into(),
            epoch,
            lease_expiry_ns,
        };

        inner.ownerships.insert(key, ownership.clone());
        Ok(ownership)
    }

    pub fn compare_and_swap_owner(
        &self,
        topic: &str,
        partition: u32,
        expected_epoch: u64,
        new_region: impl Into<String>,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError> {
        let stream = StreamId::new(topic, partition);
        self.compare_and_swap_epoch(
            &stream,
            expected_epoch,
            expected_epoch + 1,
            &new_region.into(),
            lease_duration_ms,
        )
    }

    pub fn get_ownership(&self, topic: &str, partition: u32) -> Option<PartitionOwnership> {
        let inner = self.inner.read().unwrap();
        inner
            .ownerships
            .get(&(topic.to_string(), partition))
            .cloned()
    }

    /// Execute against a stable in-process ownership view. The read guard
    /// prevents local ownership changes during an append/ack operation. This
    /// does NOT turn this store into a distributed metadata quorum.
    pub fn with_ownership<R>(
        &self,
        topic: &str,
        partition: u32,
        operation: impl FnOnce(Option<&PartitionOwnership>) -> R,
    ) -> R {
        let inner = self.inner.read().unwrap();
        operation(inner.ownerships.get(&(topic.to_string(), partition)))
    }

    pub fn update_watermark(&self, topic: &str, partition: u32, region: &str, watermark: u64) {
        let mut inner = self.inner.write().unwrap();
        let key = (topic.to_string(), partition, region.to_string());
        if let Some(r) = inner.replicas.get_mut(&key) {
            if watermark > r.durable_watermark {
                r.durable_watermark = watermark;
            }
        } else {
            inner.replicas.insert(
                key,
                ReplicaState {
                    topic: topic.to_string(),
                    partition,
                    region_id: region.to_string(),
                    dest_mode: "CLEARTEXT".to_string(),
                    durable_watermark: watermark,
                },
            );
        }
    }

    pub fn get_watermark(&self, topic: &str, partition: u32, region: &str) -> u64 {
        let inner = self.inner.read().unwrap();
        inner
            .replicas
            .get(&(topic.to_string(), partition, region.to_string()))
            .map(|r| r.durable_watermark)
            .unwrap_or(0)
    }

    pub fn record_outbox_stream(&self, meta: OutboxStreamMeta) {
        let mut inner = self.inner.write().unwrap();
        let key = (meta.topic.clone(), meta.partition, meta.dest_region.clone());
        inner.outbox_streams.insert(key, meta);
    }

    pub fn get_outbox_stream(
        &self,
        topic: &str,
        partition: u32,
        dest: &str,
    ) -> Option<OutboxStreamMeta> {
        let inner = self.inner.read().unwrap();
        inner
            .outbox_streams
            .get(&(topic.to_string(), partition, dest.to_string()))
            .cloned()
    }
}

impl MetadataAuthority for MetadataStore {
    fn deployment_mode(&self) -> DeploymentMode {
        self.inner.read().unwrap().mode
    }

    fn compare_and_swap_epoch(
        &self,
        stream: &StreamId,
        expected_epoch: u64,
        new_epoch: u64,
        new_owner_region: &str,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let key = (stream.topic.clone(), stream.partition);

        let current = match inner.ownerships.get(&key) {
            Some(o) => o,
            None => {
                if expected_epoch != 0 {
                    return Err(MetaError::PartitionNotFound {
                        topic: stream.topic.clone(),
                        partition: stream.partition,
                    });
                }
                let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
                let ownership = PartitionOwnership {
                    topic: stream.topic.clone(),
                    partition: stream.partition,
                    owner_region: new_owner_region.to_string(),
                    epoch: new_epoch,
                    lease_expiry_ns: now_ns + (lease_duration_ms * 1_000_000),
                };
                inner.ownerships.insert(key, ownership.clone());
                return Ok(ownership);
            }
        };

        if current.epoch != expected_epoch {
            return Err(MetaError::StaleEpoch {
                current: current.epoch,
                proposed: expected_epoch,
            });
        }

        if new_epoch <= current.epoch {
            return Err(MetaError::StaleEpoch {
                current: current.epoch,
                proposed: new_epoch,
            });
        }

        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        if current.owner_region != new_owner_region && current.is_lease_valid(now_ns) {
            return Err(MetaError::LeaseConflict {
                partition: stream.partition,
            });
        }

        let new_ownership = PartitionOwnership {
            topic: stream.topic.clone(),
            partition: stream.partition,
            owner_region: new_owner_region.to_string(),
            epoch: new_epoch,
            lease_expiry_ns: now_ns + (lease_duration_ms * 1_000_000),
        };

        inner.ownerships.insert(key, new_ownership.clone());
        Ok(new_ownership)
    }

    fn grant_lease(
        &self,
        stream: &StreamId,
        holder_region: &str,
        duration_ms: i64,
    ) -> Result<LeaseGrant, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let key = (stream.topic.clone(), stream.partition);

        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let expiry_ns = now_ns + (duration_ms * 1_000_000);

        let lease_id = inner.next_lease_id;
        inner.next_lease_id += 1;

        let epoch = {
            let ownership = match inner.ownerships.get_mut(&key) {
                Some(o) => o,
                None => {
                    return Err(MetaError::PartitionNotFound {
                        topic: stream.topic.clone(),
                        partition: stream.partition,
                    });
                }
            };
            ownership.owner_region = holder_region.to_string();
            ownership.lease_expiry_ns = expiry_ns;
            ownership.epoch
        };

        let grant = LeaseGrant {
            lease_id,
            stream: stream.clone(),
            holder_region: holder_region.to_string(),
            granted_at_ns: now_ns,
            expiry_ns,
            epoch,
        };

        inner.leases.insert(lease_id, grant.clone());
        Ok(grant)
    }

    fn renew_lease(
        &self,
        stream: &StreamId,
        lease_id: u64,
        duration_ms: i64,
    ) -> Result<LeaseGrant, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let new_expiry_ns = now_ns + (duration_ms * 1_000_000);

        let grant_clone = {
            let grant = match inner.leases.get_mut(&lease_id) {
                Some(g) if &g.stream == stream => g,
                _ => return Err(MetaError::LeaseNotFound(lease_id)),
            };
            grant.expiry_ns = new_expiry_ns;
            grant.clone()
        };

        let key = (stream.topic.clone(), stream.partition);
        if let Some(owner) = inner.ownerships.get_mut(&key) {
            owner.lease_expiry_ns = new_expiry_ns;
        }

        Ok(grant_clone)
    }

    fn revoke_lease(&self, stream: &StreamId, lease_id: u64) -> Result<RevocationAck, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let grant = inner.leases.remove(&lease_id);

        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let key = (stream.topic.clone(), stream.partition);
        if let Some(owner) = inner.ownerships.get_mut(&key) {
            owner.lease_expiry_ns = now_ns;
        } else if grant.is_none() {
            return Err(MetaError::LeaseNotFound(lease_id));
        }

        let effective_boundary_ns = grant
            .as_ref()
            .map(|g| std::cmp::max(now_ns, g.expiry_ns))
            .unwrap_or(now_ns);

        Ok(RevocationAck {
            lease_id,
            stream: stream.clone(),
            revoked_at_ns: now_ns,
            effective_boundary_ns,
        })
    }

    fn get_policy_floor(&self, cluster_id: &str) -> Result<u64, MetaError> {
        let inner = self.inner.read().unwrap();
        Ok(inner.policy_floors.get(cluster_id).copied().unwrap_or(1))
    }

    fn advance_policy_floor(&self, cluster_id: &str, new_floor: u64) -> Result<u64, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let current = inner.policy_floors.get(cluster_id).copied().unwrap_or(1);
        if new_floor < current {
            return Err(MetaError::PolicyFloorRegression {
                current,
                proposed: new_floor,
            });
        }
        inner
            .policy_floors
            .insert(cluster_id.to_string(), new_floor);
        Ok(new_floor)
    }

    fn issue_source_identity(
        &self,
        cluster_id: &str,
        node_id: &str,
    ) -> Result<SourceIdentityGrant, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let key = format!("{}:{}", cluster_id, node_id);
        let next_gen = inner.source_generations.get(&key).copied().unwrap_or(0) + 1;
        inner.source_generations.insert(key, next_gen);

        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        Ok(SourceIdentityGrant {
            cluster_id: cluster_id.to_string(),
            node_id: node_id.to_string(),
            generation: next_gen,
            issued_at_ns: now_ns,
        })
    }

    fn commit_audit_checkpoint(
        &self,
        checkpoint: AuditCheckpoint,
    ) -> Result<CheckpointReceipt, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let stream = checkpoint.stream.clone();
        let seq = checkpoint.sequence;
        let list = inner.audit_checkpoints.entry(stream.clone()).or_default();
        list.push(checkpoint);

        Ok(CheckpointReceipt {
            stream,
            checkpoint_sequence: seq,
            committed_revision: list.len() as u64,
            anchored_at_ns: now_ns,
        })
    }

    fn read_audit_checkpoint(
        &self,
        stream: &StreamId,
    ) -> Result<Option<AuditCheckpoint>, MetaError> {
        let inner = self.inner.read().unwrap();
        Ok(inner
            .audit_checkpoints
            .get(stream)
            .and_then(|list| list.last().cloned()))
    }

    fn get_ownership(&self, topic: &str, partition: u32) -> Option<PartitionOwnership> {
        self.get_ownership(topic, partition)
    }

    fn reacquire_ownership(
        &self,
        stream: &StreamId,
        region: &str,
        duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError> {
        let mut inner = self.inner.write().unwrap();
        let key = (stream.topic.clone(), stream.partition);
        let current =
            inner
                .ownerships
                .get_mut(&key)
                .ok_or_else(|| MetaError::PartitionNotFound {
                    topic: stream.topic.clone(),
                    partition: stream.partition,
                })?;
        if current.owner_region != region {
            return Err(MetaError::LeaseConflict {
                partition: stream.partition,
            });
        }
        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        current.lease_expiry_ns = now_ns + (duration_ms * 1_000_000);
        Ok(current.clone())
    }
}

impl LocalReplicationTracker for MetadataStore {
    fn update_watermark(&self, topic: &str, partition: u32, region: &str, watermark: u64) {
        self.update_watermark(topic, partition, region, watermark);
    }

    fn get_watermark(&self, topic: &str, partition: u32, region: &str) -> u64 {
        self.get_watermark(topic, partition, region)
    }

    fn record_outbox_stream(&self, meta: OutboxStreamMeta) {
        self.record_outbox_stream(meta);
    }

    fn get_outbox_stream(
        &self,
        topic: &str,
        partition: u32,
        dest: &str,
    ) -> Option<OutboxStreamMeta> {
        self.get_outbox_stream(topic, partition, dest)
    }
}

// ============================================================================
// 5. Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_local_residency_enforcement() {
        let audit = AuditLog::new_in_memory();
        let engine = KamishPolicyEngine::new(audit.clone());

        let event = StreamEvent::new(
            1,
            "patient-data",
            "patient-42",
            RegionId::EuWest,
            ResidencyPolicy::StrictLocal,
            b"classified-health-record".to_vec(),
        );

        // 1. Ingestion in EuWest -> OK
        assert!(engine.validate_ingestion(&event, &RegionId::EuWest).is_ok());

        // 2. Replication to UsEast -> MUST FAIL
        let violation = engine
            .validate_replication(&event, &RegionId::EuWest, &RegionId::UsEast)
            .unwrap_err();
        assert!(matches!(
            violation,
            PolicyViolation::ReplicationDenied { .. }
        ));

        // 3. Verifies auditable security rejection log entry was recorded
        assert_eq!(audit.count(), 2);
        let entries = audit.entries();
        assert_eq!(entries[1].outcome, DecisionOutcome::Deny);
        assert!(audit.verify_integrity().is_ok());
    }

    #[test]
    fn test_restricted_replication_policy() {
        let audit = AuditLog::new_in_memory();
        let engine = KamishPolicyEngine::new(audit.clone());

        let event = StreamEvent::new(
            2,
            "financial-tx",
            "tx-99",
            RegionId::EuWest,
            ResidencyPolicy::RestrictedReplication(vec![RegionId::EuWest, RegionId::ApSouth]),
            b"sovereign-transfer".to_vec(),
        );

        // Allowed to AP_SOUTH
        assert!(
            engine
                .validate_replication(&event, &RegionId::EuWest, &RegionId::ApSouth)
                .is_ok()
        );

        // Denied to US_EAST
        let err = engine
            .validate_replication(&event, &RegionId::EuWest, &RegionId::UsEast)
            .unwrap_err();
        assert!(matches!(err, PolicyViolation::ReplicationDenied { .. }));
    }

    #[test]
    fn test_replication_lag_metric_calculation() {
        let metrics = ReplicationMetricsTracker::new();
        metrics.set_local_committed("orders", 0, 1000);
        metrics.set_remote_acked("orders", 0, RegionId::UsEast, 850);

        // Lag = 1000 - 850 = 150
        assert_eq!(
            metrics.get_replication_lag("orders", 0, &RegionId::UsEast),
            150
        );
    }

    #[tokio::test]
    async fn test_synthetic_chaos_delay_and_partition() {
        let chaos = SyntheticChaosInjector::new();
        chaos.set_delay(5);
        assert!(chaos.simulate_link_traversal().await.is_ok());

        chaos.set_partition(true);
        let err = chaos.simulate_link_traversal().await.unwrap_err();
        assert_eq!(err, "NETWORK_PARTITION: Link severed");
    }

    #[test]
    fn test_metadata_authority_cas_and_leases() {
        let authority: Arc<dyn MetadataAuthority> = Arc::new(MetadataStore::new());
        assert_eq!(
            authority.deployment_mode(),
            DeploymentMode::SingleNodeCompatibility
        );

        let stream = StreamId::new("orders", 0);
        // Single-node mode CAS allows creation at expected_epoch 0
        let created = authority
            .compare_and_swap_epoch(&stream, 0, 1, "region-a", 10_000)
            .unwrap();
        assert_eq!(created.epoch, 1);
        assert_eq!(created.owner_region, "region-a");

        // Stale CAS fails
        let stale = authority.compare_and_swap_epoch(&stream, 0, 2, "region-b", 5000);
        assert!(stale.is_err());

        // Transfer to region-b while region-a lease is active fails with LeaseConflict
        let conflict = authority.compare_and_swap_epoch(&stream, 1, 2, "region-b", 10_000);
        assert!(matches!(conflict, Err(MetaError::LeaseConflict { .. })));

        // After prior lease has expired, transfer to region-b succeeds
        let stream_expired = StreamId::new("orders-expired", 0);
        authority
            .compare_and_swap_epoch(&stream_expired, 0, 1, "region-a", -1000)
            .unwrap();
        let advanced = authority
            .compare_and_swap_epoch(&stream_expired, 1, 2, "region-b", 10_000)
            .unwrap();
        assert_eq!(advanced.epoch, 2);
        assert_eq!(advanced.owner_region, "region-b");

        // Grant lease on advanced stream
        let lease = authority
            .grant_lease(&stream_expired, "region-b", 2000)
            .unwrap();
        assert_eq!(lease.holder_region, "region-b");
        assert_eq!(lease.epoch, 2);

        // Renew lease
        let renewed = authority
            .renew_lease(&stream_expired, lease.lease_id, 5000)
            .unwrap();
        assert!(renewed.expiry_ns > lease.expiry_ns);

        // Revoke lease
        let ack = authority
            .revoke_lease(&stream_expired, lease.lease_id)
            .unwrap();
        assert_eq!(ack.lease_id, lease.lease_id);
    }

    #[test]
    fn test_metadata_authority_policy_floor_and_source_identity() {
        let authority = MetadataStore::new();
        assert_eq!(authority.get_policy_floor("c1").unwrap(), 1);
        assert_eq!(authority.advance_policy_floor("c1", 3).unwrap(), 3);
        assert!(authority.advance_policy_floor("c1", 2).is_err());

        let id1 = authority.issue_source_identity("c1", "node-1").unwrap();
        assert_eq!(id1.generation, 1);
        let id2 = authority.issue_source_identity("c1", "node-1").unwrap();
        assert_eq!(id2.generation, 2);
    }

    #[test]
    fn test_metadata_authority_audit_checkpoints() {
        let authority = MetadataStore::new();
        let stream = StreamId::new("audit-test", 0);

        let cp1 = AuditCheckpoint {
            stream: stream.clone(),
            sequence: 1,
            tail_hash: "hash1".to_string(),
            record_count: 50,
            signature_hex: "sig1".to_string(),
            witness_timestamp_ns: 1000,
        };

        let receipt = authority.commit_audit_checkpoint(cp1).unwrap();
        assert_eq!(receipt.checkpoint_sequence, 1);
        assert_eq!(receipt.committed_revision, 1);

        let retrieved = authority.read_audit_checkpoint(&stream).unwrap().unwrap();
        assert_eq!(retrieved.sequence, 1);
        assert_eq!(retrieved.tail_hash, "hash1");
        assert_eq!(retrieved.record_count, 50);
    }

    #[test]
    fn test_metadata_authority_multi_region_cannot_set_ownership_except_through_cas() {
        let authority = MetadataStore::new_multi_region();
        assert_eq!(authority.deployment_mode(), DeploymentMode::MultiRegion);

        // 1. Calling set_ownership directly in MultiRegion mode MUST fail
        let direct_res = authority.set_ownership("unregistered-topic", 0, "region-a", 1, 10_000);
        assert!(
            matches!(direct_res, Err(MetaError::AuthorityError(ref msg)) if msg.contains("MultiRegion mode cannot set ownership except through compare_and_swap_epoch")),
            "Expected InvalidState error, got: {:?}",
            direct_res
        );

        let stream = StreamId::new("unregistered-topic", 0);

        // 2. CAS with non-zero expected epoch on unregistered stream fails with PartitionNotFound
        let stale_cas = authority.compare_and_swap_epoch(&stream, 5, 6, "region-a", 10_000);
        assert!(matches!(
            stale_cas,
            Err(MetaError::PartitionNotFound { .. })
        ));

        // 3. Registering ownership through compare_and_swap_epoch at expected_epoch 0 succeeds
        let initial_res = authority
            .compare_and_swap_epoch(&stream, 0, 1, "region-a", 10_000)
            .expect("CAS registration of initial epoch must succeed");
        assert_eq!(initial_res.owner_region, "region-a");
        assert_eq!(initial_res.epoch, 1);

        // 4. Ownership is now visible
        let active = authority
            .get_ownership(&stream.topic, stream.partition)
            .unwrap();
        assert_eq!(active.owner_region, "region-a");
        assert_eq!(active.epoch, 1);

        // 5. Competing claimant trying to claim epoch 1 with expected_epoch 0 fails with StaleEpoch
        let competing = authority.compare_and_swap_epoch(&stream, 0, 1, "region-b", 10_000);
        assert!(matches!(
            competing,
            Err(MetaError::StaleEpoch {
                current: 1,
                proposed: 0
            })
        ));
    }

    #[test]
    fn test_lease_monotonic_clock_pause_skew_and_expiry() {
        let clock = Arc::new(MockMonotonicClock::new(1000));
        let stream = StreamId::new("orders", 0);

        let grant = LeaseGrant {
            lease_id: 42,
            stream: stream.clone(),
            holder_region: "region-a".to_string(),
            granted_at_ns: 0,
            expiry_ns: 5_000_000_000, // 5000 ms duration
            epoch: 3,
        };

        // Safety allowance: 200 ms max clock drift + 100 ms network transit = 300 ms
        let lease = LocalLease::from_grant(&grant, clock.now_monotonic_ms(), 200, 100);
        // Effective duration = 5000 - 300 = 4700 ms
        assert_eq!(lease.effective_duration_ms, 4700);

        // Immediately valid
        assert!(lease.is_valid(clock.now_monotonic_ms()));
        assert_eq!(lease.remaining_ms(clock.now_monotonic_ms()), 4700);

        // Advance 2000 ms (normal operation)
        clock.advance_ms(2000);
        assert!(lease.is_valid(clock.now_monotonic_ms()));
        assert_eq!(lease.remaining_ms(clock.now_monotonic_ms()), 2700);

        // Simulate client/thread pause (e.g. GC or deschedule) of 2800 ms
        clock.advance_ms(2800); // total elapsed 4800 ms > 4700 ms
        assert!(!lease.is_valid(clock.now_monotonic_ms()));
        assert_eq!(lease.remaining_ms(clock.now_monotonic_ms()), 0);

        // Wall-clock backward step does NOT resurrect expired lease because monotonic clock only moves forward
        assert!(!lease.is_valid(clock.now_monotonic_ms()));
    }
}
