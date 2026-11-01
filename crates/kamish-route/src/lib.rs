//! WAN-Adaptive Route Selector with Hard Policy Filtering and Hysteresis.
//!
//! Complies with `project.md`:
//! - §2.2: "The route selector filters to the policy-permitted graph first, then optimises inside it."
//! - §2.7.4: Level 3 Process 4.3 Select Route.
//! - §3.3.4: Route Circuit Breaker State Machine.
//! - §3.4.3: Reference Pseudocode for Route Scoring with Hard Policy Filter.
//! - NFR-04: Route changes require smoothed, windowed evidence.

use kamish_audit::AuditAction;
use kamish_enforce::{EnforcementRequest, Enforcer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

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

/// Online signal tracker computing Exponentially Weighted Moving Average (EWMA) (W26).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkMetricTracker {
    pub smoothed_rtt_ms: f64,
    pub smoothed_loss_rate: f64,
    pub smoothed_queue_delay_ms: f64,
    pub cost_weight: f64,
    pub congestion_risk: f64,
    pub alpha: f64,
    pub sample_count: usize,
}

impl LinkMetricTracker {
    pub fn new(alpha: f64) -> Self {
        Self {
            smoothed_rtt_ms: 10.0,
            smoothed_loss_rate: 0.0,
            smoothed_queue_delay_ms: 2.0,
            cost_weight: 1.0,
            congestion_risk: 0.0,
            alpha: alpha.clamp(0.01, 1.0),
            sample_count: 0,
        }
    }

    /// Records an active network observation and computes smoothed signals via EWMA (W26).
    pub fn record_observation(&mut self, rtt_ms: f64, lost: bool, queue_delay_ms: f64) {
        let loss_sample = if lost { 1.0 } else { 0.0 };
        if self.sample_count == 0 {
            self.smoothed_rtt_ms = rtt_ms;
            self.smoothed_loss_rate = loss_sample;
            self.smoothed_queue_delay_ms = queue_delay_ms;
        } else {
            let a = self.alpha;
            self.smoothed_rtt_ms = (a * rtt_ms) + ((1.0 - a) * self.smoothed_rtt_ms);
            self.smoothed_loss_rate = (a * loss_sample) + ((1.0 - a) * self.smoothed_loss_rate);
            self.smoothed_queue_delay_ms =
                (a * queue_delay_ms) + ((1.0 - a) * self.smoothed_queue_delay_ms);
        }
        self.sample_count += 1;
    }

    /// Returns smoothed LinkMetrics for route cost calculation.
    pub fn metrics(&self) -> LinkMetrics {
        LinkMetrics {
            rtt_ms: self.smoothed_rtt_ms,
            loss_rate: self.smoothed_loss_rate,
            queue_delay_ms: self.smoothed_queue_delay_ms,
            cost_weight: self.cost_weight,
            congestion_risk: self.congestion_risk,
        }
    }
}

/// Circuit Breaker State Machine per network link (W27, §3.3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct CircuitBreaker {
    pub state: CircuitBreakerState,
    pub consecutive_failures: u32,
    pub failure_threshold: u32,
    pub consecutive_successes: u32,
    pub recovery_threshold: u32,
    pub trip_instant: Option<Instant>,
    pub dwell_window: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, dwell_window: Duration) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            consecutive_failures: 0,
            failure_threshold,
            consecutive_successes: 0,
            recovery_threshold: 2,
            trip_instant: None,
            dwell_window,
        }
    }

    pub fn update_state(&mut self, now: Instant) -> CircuitBreakerState {
        if self.state == CircuitBreakerState::Open
            && let Some(tripped) = self.trip_instant
            && now.duration_since(tripped) >= self.dwell_window
        {
            self.state = CircuitBreakerState::HalfOpen;
            self.consecutive_successes = 0;
        }
        self.state
    }

    pub fn is_available(&mut self, now: Instant) -> bool {
        match self.update_state(now) {
            CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen => true,
            CircuitBreakerState::Open => false,
        }
    }

    pub fn on_success(&mut self) {
        self.consecutive_failures = 0;
        if self.state == CircuitBreakerState::HalfOpen {
            self.consecutive_successes += 1;
            if self.consecutive_successes >= self.recovery_threshold {
                self.state = CircuitBreakerState::Closed;
                self.trip_instant = None;
            }
        }
    }

    pub fn on_failure(&mut self, now: Instant) {
        self.consecutive_successes = 0;
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.failure_threshold
            || self.state == CircuitBreakerState::HalfOpen
        {
            self.state = CircuitBreakerState::Open;
            self.trip_instant = Some(now);
        }
    }
}

/// Fair Bounded Retry Policy with Exponential Backoff (W27).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_millis(1000),
        }
    }
}

impl RetryPolicy {
    pub fn can_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries
    }

    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        let factor = 1u64.checked_shl(attempt.min(10)).unwrap_or(1024);
        let exponential = self.base_backoff.saturating_mul(factor as u32);
        std::cmp::min(exponential, self.max_backoff)
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
    fn same_route(&self, other: &Self) -> bool {
        self.target_region == other.target_region && self.relay_region == other.relay_region
    }

    fn valid_metrics(&self) -> bool {
        let m = self.metrics;
        [
            m.rtt_ms,
            m.loss_rate,
            m.queue_delay_ms,
            m.cost_weight,
            m.congestion_risk,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0)
            && m.loss_rate <= 1.0
            && m.congestion_risk <= 1.0
            && self.compute_cost().is_finite()
    }

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
    min_dwell: Duration,
    inner: Arc<RwLock<RouterInner>>,
}

struct RouterInner {
    current_paths: HashMap<(String, String, bool), PathCandidate>,
    last_switch: HashMap<(String, String, bool), Instant>,
    circuit_breakers: HashMap<(String, Option<String>), CircuitBreaker>,
}

impl RouteSelector {
    pub fn new(local_region: impl Into<String>, enforcer: Enforcer) -> Self {
        Self {
            local_region: local_region.into(),
            enforcer,
            hysteresis_margin: 0.15, // 15% margin required to switch paths (§3.4.3)
            min_dwell: Duration::from_secs(5),
            inner: Arc::new(RwLock::new(RouterInner {
                current_paths: HashMap::new(),
                last_switch: HashMap::new(),
                circuit_breakers: HashMap::new(),
            })),
        }
    }

    /// Picks the optimal path for a destination, enforcing hard policy filter FIRST (§2.2, §3.4.3).
    /// Records a successful transfer across a link, resetting failures or recovering half-open breaker (W27).
    pub fn record_link_success(&self, target_region: &str, relay_region: Option<&str>) {
        let mut inner = self.inner.write().unwrap();
        let key = (target_region.to_string(), relay_region.map(str::to_string));
        inner
            .circuit_breakers
            .entry(key)
            .or_insert_with(|| CircuitBreaker::new(3, Duration::from_secs(2)))
            .on_success();
    }

    /// Records a failed transfer across a link, incrementing failures and tripping breaker if threshold reached (W27).
    pub fn record_link_failure(&self, target_region: &str, relay_region: Option<&str>) {
        let now = Instant::now();
        let mut inner = self.inner.write().unwrap();
        let key = (target_region.to_string(), relay_region.map(str::to_string));
        inner
            .circuit_breakers
            .entry(key)
            .or_insert_with(|| CircuitBreaker::new(3, Duration::from_secs(2)))
            .on_failure(now);
    }

    /// Returns the current circuit breaker state for a link (W27).
    pub fn circuit_breaker_state(
        &self,
        target_region: &str,
        relay_region: Option<&str>,
    ) -> CircuitBreakerState {
        let now = Instant::now();
        let mut inner = self.inner.write().unwrap();
        let key = (target_region.to_string(), relay_region.map(str::to_string));
        inner
            .circuit_breakers
            .entry(key)
            .or_insert_with(|| CircuitBreaker::new(3, Duration::from_secs(2)))
            .update_state(now)
    }

    pub fn pick_path(
        &self,
        dest_region: &str,
        class_id: &str,
        candidates: &[PathCandidate],
    ) -> Option<PathCandidate> {
        // Callers that have not established ciphertext may only use direct paths.
        self.pick_path_for_payload(dest_region, class_id, candidates, false)
    }

    /// Route planning for a payload whose representation was established by the
    /// caller. This does not encrypt bytes or authenticate a network envelope.
    pub fn pick_path_for_payload(
        &self,
        dest_region: &str,
        class_id: &str,
        candidates: &[PathCandidate],
        is_ciphertext: bool,
    ) -> Option<PathCandidate> {
        let now = Instant::now();
        let key = (dest_region.to_string(), class_id.to_string(), is_ciphertext);

        // 1. HARD POLICY FILTER FIRST (§2.2, §2.7.4 Process 4.3.3)
        // Only evaluate candidates where every hop is legally permitted by policy
        let permitted_candidates: Vec<&PathCandidate> = candidates
            .iter()
            .filter(|cand| {
                if cand.target_region != dest_region || !cand.valid_metrics() {
                    return false;
                }
                // Check Circuit Breaker availability (W27)
                {
                    let mut inner = self.inner.write().unwrap();
                    let cb_key = (cand.target_region.clone(), cand.relay_region.clone());
                    if let Some(cb) = inner.circuit_breakers.get_mut(&cb_key)
                        && !cb.is_available(now)
                    {
                        return false;
                    }
                }
                if let Some(ref relay) = cand.relay_region {
                    if !is_ciphertext {
                        return false;
                    }
                    // Check relay transit hop (E10)
                    let transit_req = EnforcementRequest::new(
                        AuditAction::RelayTransit,
                        class_id,
                        &self.local_region,
                        relay,
                    )
                    .with_ciphertext(is_ciphertext);
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
                    && (is_ciphertext
                        || send_dec.mode != kamish_policy::DestinationMode::SealedOnly)
            })
            .collect();

        if permitted_candidates.is_empty() {
            let mut inner = self.inner.write().unwrap();
            inner.current_paths.remove(&key);
            inner.last_switch.remove(&key);
            // No legal alternate exists (§1.4.1 FR-09 per-class degradation)
            return None;
        }

        // 2. Score candidate paths inside the permitted graph (§3.4.3)
        let mut scored: Vec<(&PathCandidate, f64)> = permitted_candidates
            .into_iter()
            .map(|cand| (cand, cand.compute_cost()))
            .collect();
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));

        let best_candidate = scored[0].0.clone();
        let best_cost = scored[0].1;

        let mut inner = self.inner.write().unwrap();
        let previous = inner.current_paths.get(&key).cloned();
        // Resolve the old route against this call's freshly authorized candidates
        // and measurements. Never let hysteresis retain a revoked/missing route.
        let current = previous.as_ref().and_then(|old| {
            scored
                .iter()
                .find(|(candidate, _)| candidate.same_route(old))
                .map(|(candidate, _)| (*candidate).clone())
        });
        let chosen = if let Some(current) = current {
            let dwell = inner
                .last_switch
                .get(&key)
                .is_none_or(|last| now.duration_since(*last) >= self.min_dwell);
            if dwell && best_cost < current.compute_cost() * (1.0 - self.hysteresis_margin) {
                best_candidate
            } else {
                current
            }
        } else {
            best_candidate
        };
        if previous.as_ref().is_none_or(|old| !old.same_route(&chosen)) {
            inner.last_switch.insert(key.clone(), now);
        }
        inner.current_paths.insert(key, chosen.clone());
        Some(chosen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
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

    #[test]
    fn test_w26_ewma_metric_smoothing() {
        let mut tracker = LinkMetricTracker::new(0.2); // alpha = 0.2

        // First observation sets values directly
        tracker.record_observation(100.0, false, 10.0);
        assert_eq!(tracker.smoothed_rtt_ms, 100.0);
        assert_eq!(tracker.smoothed_loss_rate, 0.0);

        // Second observation smoothed: 0.2 * 200 + 0.8 * 100 = 120
        tracker.record_observation(200.0, true, 20.0);
        assert_eq!(tracker.smoothed_rtt_ms, 120.0);
        assert!((tracker.smoothed_loss_rate - 0.2).abs() < 1e-6);

        let metrics = tracker.metrics();
        assert_eq!(metrics.rtt_ms, 120.0);
    }

    #[test]
    fn test_w27_circuit_breaker_tripping_and_recovery() {
        let mut cb = CircuitBreaker::new(3, Duration::from_millis(50));
        let now = Instant::now();

        assert_eq!(cb.state, CircuitBreakerState::Closed);
        assert!(cb.is_available(now));

        // 3 consecutive failures trip the breaker
        cb.on_failure(now);
        cb.on_failure(now);
        cb.on_failure(now);

        assert_eq!(cb.state, CircuitBreakerState::Open);
        assert!(!cb.is_available(now));

        // Within dwell window, still open
        let soon = now + Duration::from_millis(20);
        assert!(!cb.is_available(soon));

        // After dwell window, transitions to HalfOpen
        let later = now + Duration::from_millis(60);
        assert!(cb.is_available(later));
        assert_eq!(cb.state, CircuitBreakerState::HalfOpen);

        // 2 consecutive successes in HalfOpen recover to Closed
        cb.on_success();
        assert_eq!(cb.state, CircuitBreakerState::HalfOpen);
        cb.on_success();
        assert_eq!(cb.state, CircuitBreakerState::Closed);
    }

    #[test]
    fn test_w27_route_selector_skips_tripped_circuit_breaker() {
        let router = setup_test_router();
        let paths = vec![
            PathCandidate {
                target_region: "region-eu-1".to_string(),
                relay_region: None,
                metrics: LinkMetrics {
                    rtt_ms: 10.0,
                    ..Default::default()
                },
            },
            PathCandidate {
                target_region: "region-eu-2".to_string(),
                relay_region: None,
                metrics: LinkMetrics {
                    rtt_ms: 50.0,
                    ..Default::default()
                },
            },
        ];

        // Initially region-eu-1 is best
        let p = router
            .pick_path("region-eu-1", "sovereign-class", &paths)
            .unwrap();
        assert_eq!(p.target_region, "region-eu-1");

        // Trip circuit breaker for region-eu-1
        router.record_link_failure("region-eu-1", None);
        router.record_link_failure("region-eu-1", None);
        router.record_link_failure("region-eu-1", None);
        assert_eq!(
            router.circuit_breaker_state("region-eu-1", None),
            CircuitBreakerState::Open
        );

        // Now region-eu-1 is skipped because circuit breaker is Open!
        assert!(
            router
                .pick_path("region-eu-1", "sovereign-class", &paths)
                .is_none()
        );
    }

    #[test]
    fn test_w27_fair_bounded_retry_policy() {
        let policy = RetryPolicy {
            max_retries: 3,
            base_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_millis(500),
        };

        assert!(policy.can_retry(0));
        assert!(policy.can_retry(2));
        assert!(!policy.can_retry(3));

        assert_eq!(policy.backoff_duration(0), Duration::from_millis(50));
        assert_eq!(policy.backoff_duration(1), Duration::from_millis(100));
        assert_eq!(policy.backoff_duration(2), Duration::from_millis(200));
        assert_eq!(policy.backoff_duration(4), Duration::from_millis(500)); // capped at max_backoff
    }
}
