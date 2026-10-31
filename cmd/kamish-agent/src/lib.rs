//! Kamish Inter-Region Replication Agent.
//!
//! Complies with `project.md`:
//! - §2.2: Replication agent sender and receiver.
//! - §2.7.3: Level 2 Process 4.0 Replicate Cross-Region.
//! - §2.7.4: Level 3 Process 4.3 Select Route (Permitted Graph First).
//! - §3.1.2: Cross-Region Replication with Durable Outbox and Resume.
//! - §3.4.5: Reference Outbox Transfer pseudocode.
//! - INV-5: Destination log is prefix of source log, offsets match exactly.
//! - INV-6: Retrying an applied range is a no-op.
//! - INV-8: No payload outside legal set or sealed-only mode.

use kamish_broker::{BrokerServer, ReplicationRequest};
use kamish_meta::MetadataStore;
use kamish_route::{LinkMetrics, PathCandidate, RouteSelector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Broker error: {0}")]
    Broker(#[from] kamish_broker::BrokerError),

    #[error("Target broker '{0}' not reachable or not registered")]
    TargetNotRegistered(String),

    #[error("No legal route available for class '{class_id}' to target '{target}'")]
    NoLegalRoute { class_id: String, target: String },
}

/// Real-time replication metrics exposed for observability (§4.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationLagMetric {
    pub source_region: String,
    pub target_region: String,
    pub topic: String,
    pub partition: u32,
    pub local_high_watermark: u64,
    pub remote_durable_watermark: u64,
    pub lag_records: u64,
    pub link_rtt_ms: f64,
}

/// Replication summary from a transfer pass.
#[derive(Debug, Clone, Default)]
pub struct TransferSummary {
    pub ranges_transferred: usize,
    pub bytes_transferred: usize,
    pub advanced_watermark: u64,
}

/// Cross-region replication agent coordinator.
pub struct ReplicationAgent {
    pub local_region: String,
    pub local_broker: Arc<BrokerServer>,
    pub meta: Arc<MetadataStore>,
    pub router: RouteSelector,
    peer_brokers: Arc<RwLock<HashMap<String, Arc<BrokerServer>>>>,
    link_metrics: Arc<RwLock<HashMap<String, LinkMetrics>>>,
}

impl ReplicationAgent {
    pub fn new(
        local_region: impl Into<String>,
        local_broker: Arc<BrokerServer>,
        meta: Arc<MetadataStore>,
        router: RouteSelector,
    ) -> Self {
        Self {
            local_region: local_region.into(),
            local_broker,
            meta,
            router,
            peer_brokers: Arc::new(RwLock::new(HashMap::new())),
            link_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a peer broker node for cross-region replication transfers.
    pub fn register_peer(&self, region: impl Into<String>, broker: Arc<BrokerServer>) {
        let region_str = region.into();
        self.peer_brokers.write().unwrap().insert(region_str.clone(), broker);
        self.link_metrics.write().unwrap().entry(region_str).or_default();
    }

    /// Sets simulated link metrics (RTT, packet loss) for slow-link testing (§4.2).
    pub fn set_link_metrics(&self, target_region: &str, metrics: LinkMetrics) {
        self.link_metrics.write().unwrap().insert(target_region.to_string(), metrics);
    }

    /// Performs a single replication drain pass for a target region (§2.7.3, §3.4.5).
    pub fn step_replication(&self, target_region: &str) -> Result<TransferSummary, AgentError> {
        let peer_broker = {
            let peers = self.peer_brokers.read().unwrap();
            peers
                .get(target_region)
                .cloned()
                .ok_or_else(|| AgentError::TargetNotRegistered(target_region.to_string()))?
        };

        let outbox = match self.local_broker.get_outbox(target_region) {
            Some(o) => o,
            None => return Ok(TransferSummary::default()),
        };

        let pending_ranges = outbox.next_transfer_batch(50);
        if pending_ranges.is_empty() {
            return Ok(TransferSummary::default());
        }

        let metrics = self
            .link_metrics
            .read()
            .unwrap()
            .get(target_region)
            .copied()
            .unwrap_or_default();

        let mut summary = TransferSummary::default();

        for range in pending_ranges {
            // 1. Route selection with hard policy filter first (§2.2, §3.4.3)
            let direct_candidate = PathCandidate {
                target_region: target_region.to_string(),
                relay_region: None,
                metrics,
            };

            let picked_path = self.router.pick_path(
                target_region,
                &range.class_id,
                &[direct_candidate],
            );

            let _path = match picked_path {
                Some(p) => p,
                None => {
                    warn!(
                        class = %range.class_id,
                        target = %target_region,
                        "No legal replication route permitted by policy, skipping range"
                    );
                    continue;
                }
            };

            // 2. Transfer range to target broker (E4 send / E5 receive)
            let rep_req = ReplicationRequest {
                topic: range.topic.clone(),
                partition: range.partition,
                source_region: self.local_region.clone(),
                target_region: target_region.to_string(),
                class_id: range.class_id.clone(),
                epoch: range.epoch,
                from_offset: range.start_offset,
                to_offset: range.end_offset,
                payload: range.payload.clone(),
                is_sealed: range.is_sealed,
            };

            let rep_res = peer_broker.handle_replication(rep_req)?;
            if rep_res.accepted {
                // 3. Advance local outbox watermark and trim applied range (INV-5, INV-6)
                outbox.advance_watermark(rep_res.applied_watermark);
                self.meta.update_watermark(
                    &range.topic,
                    range.partition,
                    target_region,
                    rep_res.applied_watermark,
                );

                summary.ranges_transferred += 1;
                summary.bytes_transferred += range.byte_size();
                summary.advanced_watermark = rep_res.applied_watermark;
            }
        }

        info!(
            target = %target_region,
            transferred = summary.ranges_transferred,
            watermark = summary.advanced_watermark,
            "Replication step completed"
        );

        Ok(summary)
    }

    /// Measures and returns real-time replication lag for observability (§4.4).
    pub fn get_replication_lag(
        &self,
        target_region: &str,
        topic: &str,
        partition: u32,
        local_hwm: u64,
    ) -> ReplicationLagMetric {
        let remote_watermark = match self.local_broker.get_outbox(target_region) {
            Some(o) => o.durable_watermark(),
            None => 0,
        };

        let lag = local_hwm.saturating_sub(remote_watermark);
        let rtt = self
            .link_metrics
            .read()
            .unwrap()
            .get(target_region)
            .map(|m| m.rtt_ms)
            .unwrap_or(0.0);

        ReplicationLagMetric {
            source_region: self.local_region.clone(),
            target_region: target_region.to_string(),
            topic: topic.to_string(),
            partition,
            local_high_watermark: local_hwm,
            remote_durable_watermark: remote_watermark,
            lag_records: lag,
            link_rtt_ms: rtt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use kamish_audit::AuditLog;
    use kamish_broker::ProduceRequest;
    use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy};
    use rand::rngs::OsRng;
    use std::collections::HashSet;

    fn setup_agent_environment() -> (ReplicationAgent, Arc<BrokerServer>, Arc<BrokerServer>, tempfile::TempDir, tempfile::TempDir) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert("region-eu".to_string());
        legal.insert("region-us".to_string());

        let mut modes = HashMap::new();
        modes.insert("region-eu".to_string(), DestinationMode::Cleartext);
        modes.insert("region-us".to_string(), DestinationMode::Cleartext);

        let mut classes = HashMap::new();
        classes.insert(
            "global-stream".to_string(),
            ClassRule {
                class_id: "global-stream".to_string(),
                description: "Cross region allowed".to_string(),
                legal_regions: legal,
                destination_modes: modes,
                ack_mode: AckMode::LocalMajority,
                failover_candidates: vec!["region-eu".to_string(), "region-us".to_string()],
                transit_allowed: true,
            },
        );

        let policy_doc = PolicyDocument {
            policy_id: "pol-agent".to_string(),
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
        let cache_arc = Arc::new(RwLock::new(cache));

        let audit_a = AuditLog::new_in_memory();
        let audit_b = AuditLog::new_in_memory();

        let enforcer_a = Arc::new(kamish_enforce::Enforcer::new("region-eu", cache_arc.clone(), audit_a));
        let enforcer_b = Arc::new(kamish_enforce::Enforcer::new("region-us", cache_arc, audit_b));

        let meta_a = Arc::new(MetadataStore::new());
        let meta_b = Arc::new(MetadataStore::new());

        let td_a = tempfile::tempdir().unwrap();
        let td_b = tempfile::tempdir().unwrap();

        let broker_a = Arc::new(BrokerServer::new("region-eu", enforcer_a.clone(), meta_a.clone(), vec!["region-us".to_string()], td_a.path()));
        let broker_b = Arc::new(BrokerServer::new("region-us", enforcer_b, meta_b, vec!["region-eu".to_string()], td_b.path()));

        let router = RouteSelector::new("region-eu", (*enforcer_a).clone());
        let agent = ReplicationAgent::new("region-eu", broker_a.clone(), meta_a, router);
        agent.register_peer("region-us", broker_b.clone());

        (agent, broker_a, broker_b, td_a, td_b)
    }

    #[test]
    fn test_agent_replication_and_lag_monitoring() {
        let (agent, broker_a, broker_b, _td_a, _td_b) = setup_agent_environment();

        // 1. Ingest 4 records into Broker A
        for seq in 0..4 {
            let req = ProduceRequest {
                topic: "orders".to_string(),
                partition: 0,
                producer_id: 8001,
                producer_epoch: 1,
                sequence_base: seq,
                class_id: "global-stream".to_string(),
                payload: format!("payload-{}", seq).into_bytes(),
            };
            broker_a.produce(req).unwrap();
        }

        // Check initial lag: 4 records
        let lag_metric = agent.get_replication_lag("region-us", "orders", 0, 4);
        assert_eq!(lag_metric.lag_records, 4);
        assert_eq!(lag_metric.remote_durable_watermark, 0);

        // 2. Step replication agent: drains outbox and replicates to Broker B
        let summary = agent.step_replication("region-us").unwrap();
        assert_eq!(summary.ranges_transferred, 4);
        assert_eq!(summary.advanced_watermark, 4);

        // 3. Lag is now 0!
        let lag_metric_after = agent.get_replication_lag("region-us", "orders", 0, 4);
        assert_eq!(lag_metric_after.lag_records, 0);
        assert_eq!(lag_metric_after.remote_durable_watermark, 4);

        // 4. Broker B has all 4 records stored in exact offset order (INV-5)
        let fetch_res = broker_b.fetch(kamish_broker::FetchRequest {
            topic: "orders".to_string(),
            partition: 0,
            class_id: "global-stream".to_string(),
            start_offset: 0,
            max_records: 10,
        }).unwrap();
        assert_eq!(fetch_res.records.len(), 4);
        assert_eq!(fetch_res.records[3].payload, b"payload-3");
    }

    #[test]
    fn test_agent_simulated_slow_link() {
        let (agent, broker_a, _broker_b, _td_a, _td_b) = setup_agent_environment();

        // Simulate a degraded slow link: 350ms RTT, 5% loss
        agent.set_link_metrics("region-us", LinkMetrics {
            rtt_ms: 350.0,
            loss_rate: 0.05,
            queue_delay_ms: 50.0,
            cost_weight: 1.0,
            congestion_risk: 0.4,
        });

        // Ingest into Broker A
        broker_a.produce(ProduceRequest {
            topic: "events".to_string(),
            partition: 0,
            producer_id: 9001,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: "global-stream".to_string(),
            payload: b"event-data".to_vec(),
        }).unwrap();

        // Lag check reflects the slow link RTT accurately
        let lag = agent.get_replication_lag("region-us", "events", 0, 1);
        assert_eq!(lag.lag_records, 1);
        assert_eq!(lag.link_rtt_ms, 350.0);
    }
}

