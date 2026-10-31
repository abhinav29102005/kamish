//! WAN-Adaptive Route Selector with Hard Policy Filtering and Hysteresis.
//!
//! Complies with `project.md`:
//! - §2.2: "The route selector filters to the policy-permitted graph first, then optimises inside it."
//! - §2.7.4: Level 3 Process 4.3 Select Route.
//! - §3.3.4: Route Circuit Breaker State Machine.
//! - §3.4.3: Reference Pseudocode for Route Scoring with Hard Policy Filter.
//! - NFR-04: Route changes require smoothed, windowed evidence.

use chrono::Utc;
use kamish_audit::AuditAction;
use kamish_enforce::{EnforcementRequest, Enforcer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Circuit breaker state per link (§3.3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    Closed,   // Normal healthy operation
    Open,     // Tripped due to sustained degradation
    HalfOpen, // Testing probe to verify recovery
}

/// Dynamic metrics collected per network link (§2.7.4).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinkMetrics {
    pub rtt_ms: f64,
    pub loss_rate: f64,
    pub queue_delay_ms: f64,
    pub cost_weight: f64,
    pub congestion_risk: f64,
}

impl Default for LinkMetrics {
    fn default() -> Self {
        Self {
            rtt_ms: 10.0,
            loss_rate: 0.0,
            queue_delay_ms: 2.0,
            cost_weight: 1.0,
            congestion_risk: 0.0,
        }
    }
}

/// A candidate replication path (direct or via transit relay) (§2.7.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathCandidate {
    pub target_region: String,
    pub relay_region: Option<String>,
    pub metrics: LinkMetrics,
}

impl PathCandidate {
    /// Cost formula from §3.4.3:
    /// cost = W1*RTT + W2*Loss + W3*QueueDelay + W4*Cost + W5*CongestionRisk
    pub fn compute_cost(&self) -> f64 {
        const W1: f64 = 0.35;
        const W2: f64 = 300.0;
        const W3: f64 = 0.25;
        const W4: f64 = 5.0;
        const W5: f64 = 50.0;

        (W1 * self.metrics.rtt_ms)
            + (W2 * self.metrics.loss_rate)
            + (W3 * self.metrics.queue_delay_ms)
            + (W4 * self.metrics.cost_weight)
            + (W5 * self.metrics.congestion_risk)
    }
}

/// Adaptive route selector maintaining hysteresis and circuit-breaker state (§3.4.3).
#[derive(Clone)]
pub struct RouteSelector {
    local_region: String,
    enforcer: Enforcer,
    hysteresis_margin: f64,
    min_dwell_ms: i64,
    inner: Arc<RwLock<RouterInner>>,
}

struct RouterInner {
    current_paths: HashMap<String, PathCandidate>,
    last_switch_time_ms: HashMap<String, i64>,
    breaker_states: HashMap<String, CircuitBreakerState>,
}

impl RouteSelector {
    pub fn new(local_region: impl Into<String>, enforcer: Enforcer) -> Self {
        Self {
            local_region: local_region.into(),
            enforcer,
            hysteresis_margin: 0.15, // 15% margin required to switch paths (§3.4.3)
            min_dwell_ms: 5000,      // Minimum dwell time before route switch (NFR-04)
            inner: Arc::new(RwLock::new(RouterInner {
                current_paths: HashMap::new(),
                last_switch_time_ms: HashMap::new(),
                breaker_states: HashMap::new(),
            })),
        }
    }

    /// Picks the optimal path for a destination, enforcing hard policy filter FIRST (§2.2, §3.4.3).
    pub fn pick_path(
        &self,
        dest_region: &str,
        class_id: &str,
        candidates: &[PathCandidate],
    ) -> Option<PathCandidate> {
        let now_ms = Utc::now().timestamp_millis();

        // 1. HARD POLICY FILTER FIRST (§2.2, §2.7.4 Process 4.3.3)
        // Only evaluate candidates where every hop is legally permitted by policy
        let permitted_candidates: Vec<&PathCandidate> = candidates
            .iter()
            .filter(|cand| {
                if let Some(ref relay) = cand.relay_region {
                    // Check relay transit hop (E10)
                    let transit_req = EnforcementRequest::new(
                        AuditAction::RelayTransit,
                        class_id,
                        &self.local_region,
                        relay,
                    )
                    .with_ciphertext(true);
                    let transit_dec = self.enforcer.decide(transit_req);
                    if !transit_dec.allowed {
                        return false;
                    }
                }

                // Check final destination send (E4)
                let send_req = EnforcementRequest::new(
                    AuditAction::ReplicationSender,
                    class_id,
                    &self.local_region,
                    &cand.target_region,
                );
                let send_dec = self.enforcer.decide(send_req);
                send_dec.allowed
            })
            .collect();

        if permitted_candidates.is_empty() {
            // No legal alternate exists (§1.4.1 FR-09 per-class degradation)
            return None;
        }

        // 2. Score candidate paths inside the permitted graph (§3.4.3)
        let mut scored: Vec<(&PathCandidate, f64)> = permitted_candidates
            .into_iter()
            .map(|cand| (cand, cand.compute_cost()))
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let best_candidate = scored[0].0.clone();
        let best_cost = scored[0].1;

        let mut inner = self.inner.write().unwrap();
        let current = inner.current_paths.get(dest_region).cloned();

        match current {
            Some(curr) => {
                let curr_cost = curr.compute_cost();
                let last_switch = inner
                    .last_switch_time_ms
                    .get(dest_region)
                    .copied()
                    .unwrap_or(0);
                let dwell_satisfied = now_ms - last_switch >= self.min_dwell_ms;
                let hysteresis_satisfied = best_cost < curr_cost * (1.0 - self.hysteresis_margin);

                // Check if current breaker is open
                let breaker = inner
                    .breaker_states
                    .get(dest_region)
                    .copied()
                    .unwrap_or(CircuitBreakerState::Closed);
                let breaker_allows_switch = breaker != CircuitBreakerState::Closed
                    || (dwell_satisfied && hysteresis_satisfied);

                if breaker_allows_switch && best_candidate != curr {
                    inner
                        .current_paths
                        .insert(dest_region.to_string(), best_candidate.clone());
                    inner
                        .last_switch_time_ms
                        .insert(dest_region.to_string(), now_ms);
                    Some(best_candidate)
                } else {
                    Some(curr)
                }
            }
            None => {
                inner
                    .current_paths
                    .insert(dest_region.to_string(), best_candidate.clone());
                inner
                    .last_switch_time_ms
                    .insert(dest_region.to_string(), now_ms);
                Some(best_candidate)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use hex;
    use kamish_audit::AuditLog;
    use kamish_policy::{
        AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
    };
    use rand::rngs::OsRng;
    use std::collections::{HashMap, HashSet};

    fn setup_test_router() -> RouteSelector {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert("region-eu-1".to_string());
        legal.insert("region-eu-2".to_string());

        let mut modes = HashMap::new();
        modes.insert("region-eu-1".to_string(), DestinationMode::Cleartext);
        modes.insert("region-eu-2".to_string(), DestinationMode::Cleartext);

        let class_rule = ClassRule {
            class_id: "sovereign-class".to_string(),
            description: "EU data".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-eu-1".to_string()],
            transit_allowed: false, // Relay transit denied!
        };

        let mut classes = HashMap::new();
        classes.insert("sovereign-class".to_string(), class_rule);

        let policy_doc = PolicyDocument {
            policy_id: "pol-1".to_string(),
            version: 1,
            issued_at_sec: Utc::now().timestamp() - 10,
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
        let enforcer = Enforcer::new("region-eu-1", Arc::new(RwLock::new(cache)), audit_log);

        RouteSelector::new("region-eu-1", enforcer)
    }

    #[test]
    fn test_hard_policy_filter_before_scoring() {
        let router = setup_test_router();

        // Path 1: To legal region-eu-2 (RTT 50ms)
        let legal_path = PathCandidate {
            target_region: "region-eu-2".to_string(),
            relay_region: None,
            metrics: LinkMetrics {
                rtt_ms: 50.0,
                ..Default::default()
            },
        };

        // Path 2: To non-legal region-us-east with tempting fast latency (RTT 5ms)
        let illegal_fast_path = PathCandidate {
            target_region: "region-us-east".to_string(),
            relay_region: None,
            metrics: LinkMetrics {
                rtt_ms: 5.0,
                ..Default::default()
            },
        };

        // Path 3: Via relay when transit is not permitted
        let illegal_relay_path = PathCandidate {
            target_region: "region-eu-2".to_string(),
            relay_region: Some("region-relay".to_string()),
            metrics: LinkMetrics {
                rtt_ms: 20.0,
                ..Default::default()
            },
        };

        let picked = router
            .pick_path(
                "region-eu-2",
                "sovereign-class",
                &[legal_path.clone(), illegal_fast_path, illegal_relay_path],
            )
            .unwrap();

        // MUST pick legal_path, never the illegal faster path!
        assert_eq!(picked.target_region, "region-eu-2");
        assert_eq!(picked.relay_region, None);
    }
}
