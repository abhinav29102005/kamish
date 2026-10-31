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
// 4. Metadata Entity-Relationship Store (§2.5)
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

/// Central in-memory, thread-safe Metadata Store (D4 in DFDs §2.7.2).
#[derive(Clone, Default)]
pub struct MetadataStore {
    inner: Arc<RwLock<MetadataStoreInner>>,
}

#[derive(Default)]
struct MetadataStoreInner {
    topics: HashMap<String, TopicMeta>,
    ownerships: HashMap<(String, u32), PartitionOwnership>,
    replicas: HashMap<(String, u32, String), ReplicaState>,
    outbox_streams: HashMap<(String, u32, String), OutboxStreamMeta>,
}

impl MetadataStore {
    pub fn new() -> Self {
        Self::default()
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
        let mut inner = self.inner.write().unwrap();
        let key = (topic.to_string(), partition);

        let current = match inner.ownerships.get(&key) {
            Some(o) => o,
            None => {
                return Err(MetaError::PartitionNotFound {
                    topic: topic.to_string(),
                    partition,
                });
            }
        };

        if current.epoch != expected_epoch {
            return Err(MetaError::StaleEpoch {
                current: current.epoch,
                proposed: expected_epoch,
            });
        }

        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let new_epoch = expected_epoch + 1;
        let new_ownership = PartitionOwnership {
            topic: topic.to_string(),
            partition,
            owner_region: new_region.into(),
            epoch: new_epoch,
            lease_expiry_ns: now_ns + (lease_duration_ms * 1_000_000),
        };

        inner.ownerships.insert(key, new_ownership.clone());
        Ok(new_ownership)
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
        &self, topic: &str, partition: u32,
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
}
