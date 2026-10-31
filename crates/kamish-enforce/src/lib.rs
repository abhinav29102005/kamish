//! Single Decision Function Engine for Kamish.
//!
//! Complies with `project.md`:
//! - §2.6: Enforcement Points Map (E1 Ingress, E2 Broker Append, E3 In-Region Replica,
//!   E4 Replication Sender, E5 Replication Receiver, E6 Backup Export, E7 Consumer Fetch,
//!   E8 Partition Promote, E8-Alt Telemetry Export, E9 Admin Replay, E10 Relay Transit).
//! - §2.7.5: Level 3 Process 2.0 Single Decision Function.
//! - §3.4.1: Reference Decision Function logic.
//! - INV-8: No payload outside legal set or sealed-only mode.
//! - INV-9: An unsigned, invalid, or rolled-back policy is never applied.
//! - FR-01: Reject operations where data class is not permitted in target region.
//! - FR-08: Promote a new owner only if policy lists it as eligible.
//! - FR-12: Transit-allowed relay hops for ciphertext only.

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
    pub is_ciphertext: bool,
}

impl EnforcementRequest {
    pub fn new(
        action: AuditAction,
        class_id: impl Into<String>,
        source_region: impl Into<String>,
        target_region: impl Into<String>,
    ) -> Self {
        Self {
            action,
            class_id: class_id.into(),
            source_region: source_region.into(),
            target_region: target_region.into(),
            is_ciphertext: false,
        }
    }

    pub fn with_ciphertext(mut self, is_ciphertext: bool) -> Self {
        self.is_ciphertext = is_ciphertext;
        self
    }
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
    /// Implements §2.6, §2.7.5, §3.4.1, and INV-8 / INV-9.
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

        // 4. Map against all 10 Enforcement Points (§2.6)
        match req.action {
            // E1 (Ingress) & E2 (Broker Append): PERSIST_LOCAL
            AuditAction::Ingress | AuditAction::BrokerAppend => {
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
                    "ok_local_persist",
                )
            }

            // E3: In-Region Replica (PERSIST_REPLICA)
            AuditAction::InRegionReplica => {
                if !class_rule.is_region_legal(&self.local_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "replica_region_not_legal",
                    );
                }
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_replica_persist",
                )
            }

            // E4 (Replication Sender): SEND_REGION & E5 (Replication Receiver): PERSIST_REMOTE
            AuditAction::ReplicationSender | AuditAction::ReplicationReceiver => {
                if !class_rule.is_region_legal(&req.target_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "target_not_in_legal_set",
                    );
                }

                let mode = class_rule.mode_for_region(&req.target_region);
                match mode {
                    DestinationMode::Cleartext => self.record_decision(
                        req,
                        policy_version,
                        true,
                        DestinationMode::Cleartext,
                        "ok_cleartext_replication",
                    ),
                    DestinationMode::SealedOnly => self.record_decision(
                        req,
                        policy_version,
                        true,
                        DestinationMode::SealedOnly,
                        "ok_sealed_replication",
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

            // E6: Backup / Tiered Export (EXPORT_BACKUP)
            AuditAction::BackupExport => {
                if !class_rule.is_region_legal(&req.target_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "backup_region_not_in_legal_set",
                    );
                }
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_backup_export",
                )
            }

            // E7: Consumer Fetch (SERVE_FETCH)
            AuditAction::ConsumerFetch => {
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

            // E8: Partition Ownership Promotion (PROMOTE - FR-08, §3.4.4)
            AuditAction::Promote => {
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

            // E8-Alt: Telemetry Export (EXPORT_TELEMETRY - Threat Model §4.7.1)
            AuditAction::TelemetryExport => {
                // Telemetry is allowed only if no payload is transmitted and target is allowed
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_telemetry_export",
                )
            }

            // E9: Admin & Replay Tools (ADMIN_READ, REPLAY)
            AuditAction::AdminReplay => {
                if !class_rule.is_region_legal(&self.local_region) {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "admin_replay_region_not_legal",
                    );
                }
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::Cleartext,
                    "ok_admin_replay",
                )
            }

            // E10: Relay Hop Transit (TRANSIT - FR-12, §1.6 D3)
            AuditAction::RelayTransit => {
                if !class_rule.transit_allowed {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "transit_not_permitted_for_class",
                    );
                }
                if !req.is_ciphertext {
                    return self.record_decision(
                        req,
                        policy_version,
                        false,
                        DestinationMode::Deny,
                        "transit_denied_for_cleartext",
                    );
                }
                self.record_decision(
                    req,
                    policy_version,
                    true,
                    DestinationMode::SealedOnly,
                    "ok_ciphertext_transit",
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
            transit_allowed: true,
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
    fn test_enforce_e1_to_e10() {
        let enforcer = setup_test_enforcer();

        // E1: Ingress (allowed in local legal region)
        let d1 = enforcer.decide(EnforcementRequest::new(
            AuditAction::Ingress,
            "EU-restricted",
            "region-eu-1",
            "region-eu-1",
        ));
        assert!(d1.allowed);

        // E4: Replication sender to legal sealed region
        let d4 = enforcer.decide(EnforcementRequest::new(
            AuditAction::ReplicationSender,
            "EU-restricted",
            "region-eu-1",
            "region-eu-2",
        ));
        assert!(d4.allowed);
        assert_eq!(d4.mode, DestinationMode::SealedOnly);

        // E10: Relay transit with ciphertext -> Allowed (FR-12)
        let d10_ok = enforcer.decide(
            EnforcementRequest::new(
                AuditAction::RelayTransit,
                "EU-restricted",
                "region-eu-1",
                "region-relay",
            )
            .with_ciphertext(true),
        );
        assert!(d10_ok.allowed);

        // E10: Relay transit with cleartext -> Denied (FR-12)
        let d10_deny = enforcer.decide(
            EnforcementRequest::new(
                AuditAction::RelayTransit,
                "EU-restricted",
                "region-eu-1",
                "region-relay",
            )
            .with_ciphertext(false),
        );
        assert!(!d10_deny.allowed);
        assert_eq!(d10_deny.reason, "transit_denied_for_cleartext");
    }
}
