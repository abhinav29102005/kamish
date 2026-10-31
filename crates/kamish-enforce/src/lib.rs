//! Single Decision Function Engine for Hydra Stream.
//!
//! Complies with `project.md`:
//! - §2.6: Enforcement Points Map (E1 Gateway, E2 Broker persistence, E3 Replication outbox,
//!   E4 Network receiver, E5 Consumer fetch, E8 Partition failover).
//! - §2.7.5: Level 3 Process 2.0 Single Decision Function.
//! - §3.4.1: Reference Decision Function pseudocode.
//! - INV-8: No payload outside legal set or sealed-only mode.
//! - INV-9: An unsigned, invalid, or rolled-back policy is never applied.
//! - FR-01: Reject operations where data class is not permitted in target region.
//! - FR-08: Promote a new owner only if policy lists it as eligible.

use chrono::Utc;
use kamish_audit::{AuditAction, AuditLog, DecisionOutcome};
use kamish_policy::{DestinationMode, PolicyCache};
use std::sync::{Arc, RwLock};

/// Enforcement request specifying operation, class, source, and target region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnforcementRequest {
    pub action: AuditAction,
    pub class_id: String,
    pub source_region: String,
    pub target_region: String,
}

/// Evaluation result returned to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnforcementDecision {
    pub allowed: bool,
    pub mode: DestinationMode,
    pub reason: String,
    pub policy_version: u64,
}

/// The core broker enforcement engine.
#[derive(Clone)]
pub struct Enforcer {
    local_region: String,
    policy_cache: Arc<RwLock<PolicyCache>>,
    audit_log: AuditLog,
}

impl Enforcer {
    pub fn new(
        local_region: impl Into<String>,
        policy_cache: Arc<RwLock<PolicyCache>>,
        audit_log: AuditLog,
    ) -> Self {
        Self {
            local_region: local_region.into(),
            policy_cache,
            audit_log,
        }
    }

    /// Evaluates the policy for a request and records the decision to the hash-chained audit log.
    /// Implements §3.4.1 reference logic and INV-8 / INV-9.
    pub fn decide(&self, req: EnforcementRequest) -> EnforcementDecision {
        let now_sec = Utc::now().timestamp();
        let cache = self.policy_cache.read().unwrap();

        // 1. Check if policy exists in cache
        let signed_policy = match cache.current() {
            Some(p) => p,
            None => {
                return self.record_decision(
                    req,
                    0,
                    false,
                    DestinationMode::Deny,
                    "no_policy_loaded",
                );
            }
        };

        let policy_version = signed_policy.policy.version;

        // 2. Freshness check (§3.3.3)
        if !signed_policy.is_fresh(now_sec) {
            return self.record_decision(
                req,
                policy_version,
                false,
                DestinationMode::Deny,
                "stale_policy_expired",
            );
        }

        // 3. Class rule lookup (§2.4)
        let class_rule = match signed_policy.policy.classes.get(&req.class_id) {
            Some(rule) => rule,
            None => {
                return self.record_decision(
                    req,
                    policy_version,
                    false,
                    DestinationMode::Deny,
                    "unknown_class",
                );
            }
        };

        // 4. Operation-specific residency and capability evaluation
        match req.action {
            AuditAction::Produce => {
                // Producing is allowed only if local ingestion region is in legal set
                if !class_rule.is_region_legal(&self.local_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "local_region_not_in_legal_set",
                    );
                }
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_local_produce",
                )
            }

            AuditAction::SendRegion | AuditAction::PersistRemote => {
                // Target region must be legal
                if !class_rule.is_region_legal(&req.target_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "target_not_in_legal_set",
                    );
                }

                // Resolve mode (Cleartext, SealedOnly, or Deny)
                let mode = class_rule.mode_for_region(&req.target_region);
                match mode {
                    DestinationMode::Cleartext => self.record_decision(
                        req,
                        policy_version,
                        true,
                        DestinationMode::Cleartext,
                        "ok_cleartext_send",
                    ),
                    DestinationMode::SealedOnly => self.record_decision(
                        req,
                        policy_version,
                        true,
                        DestinationMode::SealedOnly,
                        "ok_sealed_send",
                    ),
                    DestinationMode::Deny => self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "destination_mode_denied",
                    ),
                }
            }

            AuditAction::Promote => {
                // Partition promotion eligibility (FR-08, §3.4.4)
                if !class_rule.is_failover_eligible(&req.target_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "region_not_failover_eligible",
                    );
                }
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_promotion_eligible",
                )
            }

            AuditAction::Fetch => {
                // Consumers can only fetch in regions permitted for cleartext access
                if !class_rule.is_region_legal(&self.local_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "fetch_region_not_legal",
                    );
                }
                let mode = class_rule.mode_for_region(&self.local_region);
                if mode != DestinationMode::Cleartext {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "fetch_requires_cleartext_mode",
                    );
                }
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_fetch",
                )
            }

            AuditAction::Replay => {
                // Replay auditing
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_replay",
                )
            }
        }
    }

    fn record_decision(
        &self,
        req: EnforcementRequest,
        policy_version: u64,
        allowed: bool,
        mode: DestinationMode,
        reason: &str,
    ) -> EnforcementDecision {
        let outcome = if !allowed {
            DecisionOutcome::Deny
        } else {
            match mode {
                DestinationMode::Cleartext => DecisionOutcome::AllowCleartext,
                DestinationMode::SealedOnly => DecisionOutcome::AllowSealed,
                DestinationMode::Deny => DecisionOutcome::Deny,
            }
        };

        // Append to hash-chained decision log (FR-13)
        let _ = self.audit_log.record(
            req.action,
            &req.class_id,
            &req.source_region,
            &req.target_region,
            policy_version,
            outcome,
            reason,
        );

        EnforcementDecision {
            allowed,
            mode,
            reason: reason.to_string(),
            policy_version,
        }
    }

    pub fn audit_log(&self) -> &AuditLog {
        &self.audit_log
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use kamish_policy::{AckMode, ClassRule, DestinationMode, PolicyDocument, SignedPolicy};
    use rand::rngs::OsRng;
    use std::collections::{HashMap, HashSet};

    fn setup_test_enforcer() -> Enforcer {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert("region-eu-1".to_string());
        legal.insert("region-eu-2".to_string());

        let mut modes = HashMap::new();
        modes.insert("region-eu-1".to_string(), DestinationMode::Cleartext);
        modes.insert("region-eu-2".to_string(), DestinationMode::SealedOnly);

        let class_rule = ClassRule {
            class_id: "EU-restricted".to_string(),
            description: "Strict sovereign data".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-eu-1".to_string(), "region-eu-2".to_string()],
            transit_allowed: false,
        };

        let mut classes = HashMap::new();
        classes.insert("EU-restricted".to_string(), class_rule);

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
        Enforcer::new("region-eu-1", Arc::new(RwLock::new(cache)), audit_log)
    }

    #[test]
    fn test_enforce_produce_and_send_rules() {
        let enforcer = setup_test_enforcer();

        // 1. Local produce in region-eu-1 -> Allow Cleartext
        let d1 = enforcer.decide(EnforcementRequest {
            action: AuditAction::Produce,
            class_id: "EU-restricted".to_string(),
            source_region: "region-eu-1".to_string(),
            target_region: "region-eu-1".to_string(),
        });
        assert!(d1.allowed);
        assert_eq!(d1.mode, DestinationMode::Cleartext);

        // 2. Send to region-eu-2 -> Allow SealedOnly
        let d2 = enforcer.decide(EnforcementRequest {
            action: AuditAction::SendRegion,
            class_id: "EU-restricted".to_string(),
            source_region: "region-eu-1".to_string(),
            target_region: "region-eu-2".to_string(),
        });
        assert!(d2.allowed);
        assert_eq!(d2.mode, DestinationMode::SealedOnly);

        // 3. Send to non-legal region-us-east -> Deny
        let d3 = enforcer.decide(EnforcementRequest {
            action: AuditAction::SendRegion,
            class_id: "EU-restricted".to_string(),
            source_region: "region-eu-1".to_string(),
            target_region: "region-us-east".to_string(),
        });
        assert!(!d3.allowed);
        assert_eq!(d3.mode, DestinationMode::Deny);
        assert_eq!(d3.reason, "target_not_in_legal_set");

        // Verify audit log has 3 chained entries with mathematical integrity
        assert_eq!(enforcer.audit_log().count(), 3);
        assert_eq!(enforcer.audit_log().verify_integrity().unwrap(), 3);
    }

    #[test]
    fn test_enforce_promotion_eligibility() {
        let enforcer = setup_test_enforcer();

        // region-eu-2 is in failover_candidates -> Allowed
        let d1 = enforcer.decide(EnforcementRequest {
            action: AuditAction::Promote,
            class_id: "EU-restricted".to_string(),
            source_region: "region-eu-1".to_string(),
            target_region: "region-eu-2".to_string(),
        });
        assert!(d1.allowed);

        // region-us-east is NOT in failover_candidates -> Denied (FR-08)
        let d2 = enforcer.decide(EnforcementRequest {
            action: AuditAction::Promote,
            class_id: "EU-restricted".to_string(),
            source_region: "region-eu-1".to_string(),
            target_region: "region-us-east".to_string(),
        });
        assert!(!d2.allowed);
        assert_eq!(d2.reason, "region_not_failover_eligible");
    }
}
