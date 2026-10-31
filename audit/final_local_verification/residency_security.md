# HydraStream — Regional Residency & Security Boundaries Audit

**Audit Phase:** Phase E — Verify Regional Residency and Security Boundaries  
**Date:** 2026-10-31  
**Auditor:** Senior Distributed Systems Engineer & Reliability Auditor  
**Artifact Path:** `audit/final_local_verification/residency_security.md`  

---

## 1. Executive Summary

HydraStream enforces regional data residency and sovereignty boundaries via a multi-layered defense architecture:
1. **Cryptographically Signed Policy Documents:** ED25519-signed policy documents with cryptographic expiration (TTL) and monotonic policy versions (`crates/kamish-policy`).
2. **Dual-Hook Interception Pipeline:** Both Ingress (`validate_ingestion` / `BrokerAppend`) and Egress (`validate_replication` / `ReplicationSender`) evaluate policies in the hot transfer path before and during replication staging (`cmd/kamish-broker`).
3. **Fail-Closed Enforcer:** If a policy is missing, corrupted, signature-tampered, or expired, all operations fail closed to `DestinationMode::Deny` (`crates/kamish-enforce`).
4. **Zero-Payload Cryptographic Audit Ledger:** Decision outcomes are appended to a tamper-evident SHA-256 hash chain without recording sensitive payload bytes (`crates/kamish-audit`).

---

## 2. Security Invariant Matrix

| # | Security Invariant | Mechanism & Enforcement Point | Verification Status | Test Evidence |
| :--- | :--- | :--- | :--- | :--- |
| **SEC-01** | Unauthorized cross-region replication is rejected | `Enforcer::decide` evaluates `legal_regions` set; foreign regions return `target_not_in_legal_set` | `PASS — independently verified` | `red_team_residency.rs::test_unauthorized_destination_egress_denial` |
| **SEC-02** | Policy checks apply to actual data transfer path | Evaluated inside `BrokerServer::produce` (L341-359) and `repopulate_outbox_from_log` (L587-595) | `PASS — independently verified` | `cmd/kamish-broker/tests/admission_control_race.rs` |
| **SEC-03** | Strict-local policy cannot be bypassed | Ingress hooks reject unauthorized ingress; egress filters drop unapproved destinations before outbox enqueue | `PASS — independently verified` | `developer_b_scenarios.rs::test_scenario_a_residency_enforcement_demo` |
| **SEC-04** | Tampered or expired policies fail closed | ED25519 signature verification fails; TTL expiry returns `stale_policy_expired` | `PASS — independently verified` | `red_team_residency.rs::test_invalid_signature_and_tampered_policy_rejection`<br>`test_expired_policy_denies_operations` |
| **SEC-05** | Rejected transfers cause zero state mutation | Dropped before local log append or outbox commit; session manager unmutated | `PASS — independently verified` | `reservation_lifecycle_and_crash.rs::test_reservation_lifecycle_and_accounting_invariants` |
| **SEC-06** | Zero-payload tamper-evident audit logging | SHA-256 hash-chained entries record action, timestamp, and rule ID with zero payload bytes | `PASS — independently verified` | `red_team_residency.rs::test_audit_ledger_zero_payload_disclosure` |
| **SEC-07** | Reconnect and retry paths re-enforce policy | `repopulate_outbox_from_log` re-runs `EnforcementRequest::new(AuditAction::ReplicationSender)` | `PASS — independently verified` | `cmd/kamish-broker/src/lib.rs:588-595` |
| **SEC-08** | Missing or error states fail closed | Missing policy returns `no_policy_loaded` with `DestinationMode::Deny` | `PASS — independently verified` | `red_team_residency.rs::test_missing_policy_denies_all_operations_closed` |

---

## 3. Ingress & Egress Code Inspection Details

### 3.1 Live Produce Ingress Interception
In [`cmd/kamish-broker/src/lib.rs`](file:///E:/Kamish/cmd/kamish-broker/src/lib.rs#L278-L314):
1. Calls `engine.validate_ingestion(&stream_event, &self.region_id)`. If rejected, returns `ProduceResponse { sovereignty_allowed: false, denial_reason: violation }`.
2. Evaluates `self.enforcer.decide(EnforcementRequest::new(AuditAction::BrokerAppend, ...))`. If not allowed, returns immediate denial without disk I/O.

### 3.2 Egress Replication Planning
In [`cmd/kamish-broker/src/lib.rs`](file:///E:/Kamish/cmd/kamish-broker/src/lib.rs#L341-L365):
```rust
for peer in &self.known_peers {
    let target_region_id = parse_region_id(peer);
    if let Some(ref engine) = self.policy_engine {
        if engine.validate_replication(&stream_event, &self.region_id, &target_region_id).is_err() {
            continue; // Prohibited by PolicyEngine contract
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
        let outbox = self.get_or_create_outbox(peer);
        // Stage reservation...
    }
}
```
If a message is `StrictLocal(EU_CENTRAL)`, replication to `US_EAST` is filtered out. The outbox for `US_EAST` is never even asked to reserve bytes, and the message never leaves the origin region.

### 3.3 Zero-Payload Audit Ledger
Audit entries record:
- Entry ID & Sequence Number
- Timestamp (UTC)
- Action (`BrokerAppend`, `ReplicationSender`, `ConsumerFetch`)
- Class ID & Region Pair
- Decision Outcome (`Allow` / `Deny`)
- Cryptographic SHA-256 hash of previous entry (Hash Chain)
Sensitive message payload bytes are excluded from the audit log, preventing data leaks through audit log ingestion.

---

## 4. Phase E Verdict

The regional data residency enforcement and security boundaries are fully operative, fail-closed, and cryptographically verified.

**Phase E Verdict: PASS — INDEPENDENTLY VERIFIED**

