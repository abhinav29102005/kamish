//! Partition Ownership, Lease Fencing, and Quarantine Segment Store for Kamish.
//!
//! Complies with `project.md`:
//! - §3.1.3: Partition Promotion after Regional Outage.
//! - §3.1.4: Old Owner Reconnects (Split-Brain Prevention and Quarantine).
//! - §3.3.1: Partition Ownership State Machine.
//! - §3.4.4: Reference Promotion Logic.
//! - INV-3: A new leader holds all acknowledged records.
//! - INV-7: No two regions commit writes for the same partition and epoch.
//! - §4.7.2 (R6): Conservative lease margins and clock drift bounds.

use chrono::Utc;
use kamish_audit::AuditAction;
use kamish_enforce::{EnforcementRequest, Enforcer};
use kamish_log::RecordEnvelope;
use kamish_meta::{MetadataStore, PartitionOwnership};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OwnershipError {
    #[error("Lease not expired yet (clock drift buffer active): expiry {expiry_ns}, now {now_ns}")]
    LeaseNotExpired { expiry_ns: i64, now_ns: i64 },

    #[error("Candidate region '{0}' is not eligible for promotion by policy")]
    NotEligible(String),

    #[error("Node does not hold required replica up to watermark")]
    NoReplica,

    #[error("Metadata conflict or stale epoch during CAS: {0}")]
    MetaConflict(String),

    #[error("Stale writer fenced: request epoch {request_epoch} < partition epoch {current_epoch}")]
    StaleWriterFenced {
        request_epoch: u64,
        current_epoch: u64,
    },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Local quarantine store for superseded log suffixes (§1.7, §3.1.4).
/// Records in quarantine are strictly isolated and NEVER replicated to peers.
#[derive(Clone)]
pub struct QuarantineStore {
    dir: PathBuf,
    inner: Arc<RwLock<Vec<RecordEnvelope>>>,
}

impl QuarantineStore {
    pub fn open<P: AsRef<Path>>(dir: P) -> Result<Self, std::io::Error> {
        let dir_buf = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir_buf)?;

        let q_file = dir_buf.join("quarantine.data");
        let mut quarantined = Vec::new();

        if q_file.exists() {
            let mut file = File::open(&q_file)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            if !buf.is_empty()
                && let Ok(records) = serde_json::from_slice::<Vec<RecordEnvelope>>(&buf)
            {
                quarantined = records;
            }
        }

        Ok(Self {
            dir: dir_buf,
            inner: Arc::new(RwLock::new(quarantined)),
        })
    }

    /// Moves a superseded suffix of records to the quarantine store (§3.1.4).
    pub fn quarantine_records(&self, records: Vec<RecordEnvelope>) -> Result<(), std::io::Error> {
        if records.is_empty() {
            return Ok(());
        }

        std::fs::create_dir_all(&self.dir)?;
        let mut inner = self.inner.write().unwrap();
        inner.extend(records);

        let q_file = self.dir.join("quarantine.data");
        let serialized = serde_json::to_vec(&*inner)?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(q_file)?;
        file.write_all(&serialized)?;
        file.flush()?;

        Ok(())
    }

    pub fn count(&self) -> usize {
        self.inner.read().unwrap().len()
    }
}

/// Partition ownership coordinator enforcing leases, fencing, and promotion (§3.4.4).
#[derive(Clone)]
pub struct PartitionOwnerCoordinator {
    local_region: String,
    max_clock_drift_ms: i64,
    meta_store: MetadataStore,
    enforcer: Enforcer,
    quarantine: QuarantineStore,
}

impl PartitionOwnerCoordinator {
    pub fn new(
        local_region: impl Into<String>,
        max_clock_drift_ms: i64,
        meta_store: MetadataStore,
        enforcer: Enforcer,
        quarantine: QuarantineStore,
    ) -> Self {
        Self {
            local_region: local_region.into(),
            max_clock_drift_ms,
            meta_store,
            enforcer,
            quarantine,
        }
    }

    /// Attempts failover promotion for a partition following §3.4.4 reference logic:
    /// 1. Verifies lease expiration + clock drift buffer.
    /// 2. Verifies policy eligibility via Enforcer (E8).
    /// 3. Commits bumped epoch through the metadata quorum via CAS.
    pub fn try_promote(
        &self,
        topic: &str,
        partition: u32,
        class_id: &str,
        holds_replica: bool,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, OwnershipError> {
        let current_ownership = match self.meta_store.get_ownership(topic, partition) {
            Some(o) => o,
            None => {
                return Err(OwnershipError::MetaConflict(
                    "Partition not found in metadata".into(),
                ));
            }
        };

        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let safe_expiry_ns =
            current_ownership.lease_expiry_ns + (self.max_clock_drift_ms * 1_000_000);

        // 1. Lease safety check with clock drift margin (§4.7.2 R6, §3.4.4)
        if now_ns < safe_expiry_ns {
            return Err(OwnershipError::LeaseNotExpired {
                expiry_ns: safe_expiry_ns,
                now_ns,
            });
        }

        // 2. Policy eligibility check via single decision function (FR-08, E8)
        let req = EnforcementRequest::new(
            AuditAction::Promote,
            class_id,
            &current_ownership.owner_region,
            &self.local_region,
        );
        let decision = self.enforcer.decide(req);
        if !decision.allowed {
            return Err(OwnershipError::NotEligible(self.local_region.clone()));
        }

        // 3. Replicate safety check (must hold committed replica up to watermark) (INV-3)
        if !holds_replica {
            return Err(OwnershipError::NoReplica);
        }

        // 4. Commit new epoch (epoch + 1) through metadata quorum CAS (INV-7)
        self.meta_store
            .compare_and_swap_owner(
                topic,
                partition,
                current_ownership.epoch,
                &self.local_region,
                lease_duration_ms,
            )
            .map_err(|e| OwnershipError::MetaConflict(e.to_string()))
    }

    /// Enforces writer fencing (§3.4.2, §3.1.4):
    /// Rejects appends if incoming epoch is lower than current committed partition epoch.
    pub fn verify_write_epoch(
        &self,
        topic: &str,
        partition: u32,
        write_epoch: u64,
    ) -> Result<(), OwnershipError> {
        if let Some(ownership) = self.meta_store.get_ownership(topic, partition)
            && write_epoch < ownership.epoch
        {
            return Err(OwnershipError::StaleWriterFenced {
                request_epoch: write_epoch,
                current_epoch: ownership.epoch,
            });
        }
        Ok(())
    }

    /// Handles a returning old leader whose log has diverged (§3.1.4):
    /// Truncates the superseded local suffix and moves it into the local quarantine store.
    pub fn reconcile_divergent_suffix(
        &self,
        superseded_records: Vec<RecordEnvelope>,
    ) -> Result<usize, OwnershipError> {
        let count = superseded_records.len();
        self.quarantine.quarantine_records(superseded_records)?;
        Ok(count)
    }

    pub fn quarantine(&self) -> &QuarantineStore {
        &self.quarantine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use kamish_audit::AuditLog;
    use kamish_policy::{
        AckMode, ClassRule, DestinationMode, PolicyCache, PolicyDocument, SignedPolicy,
    };
    use rand::rngs::OsRng;
    use std::collections::{HashMap, HashSet};

    struct TestFixture {
        _temp_dir: tempfile::TempDir,
        coordinator: PartitionOwnerCoordinator,
    }

    fn setup_test_fixture(now_offset_ms: i64) -> TestFixture {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mut legal = HashSet::new();
        legal.insert("region-a".to_string());
        legal.insert("region-b".to_string());

        let mut modes = HashMap::new();
        modes.insert("region-a".to_string(), DestinationMode::Cleartext);
        modes.insert("region-b".to_string(), DestinationMode::Cleartext);

        let class_rule = ClassRule {
            class_id: "failover-class".to_string(),
            description: "Failover test data".to_string(),
            legal_regions: legal,
            destination_modes: modes,
            ack_mode: AckMode::LocalMajority,
            failover_candidates: vec!["region-a".to_string(), "region-b".to_string()],
            transit_allowed: false,
        };

        let mut classes = HashMap::new();
        classes.insert("failover-class".to_string(), class_rule);

        let policy_doc = PolicyDocument {
            policy_id: "pol-failover".to_string(),
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
        let enforcer = Enforcer::new("region-b", Arc::new(RwLock::new(cache)), audit_log);

        let meta = MetadataStore::new();
        meta.create_topic("orders", "failover-class", 1);
        meta.set_ownership("orders", 0, "region-a", 1, now_offset_ms)
            .unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        let quarantine = QuarantineStore::open(temp_dir.path()).unwrap();

        let coordinator =
            PartitionOwnerCoordinator::new("region-b", 50, meta, enforcer, quarantine);
        TestFixture {
            _temp_dir: temp_dir,
            coordinator,
        }
    }

    #[test]
    fn test_promotion_before_lease_expires_fails() {
        let fixture = setup_test_fixture(10_000);
        let err = fixture
            .coordinator
            .try_promote("orders", 0, "failover-class", true, 5000)
            .unwrap_err();
        assert!(matches!(err, OwnershipError::LeaseNotExpired { .. }));
    }

    #[test]
    fn test_promotion_after_lease_expires_succeeds() {
        let fixture = setup_test_fixture(-1000);
        let new_ownership = fixture
            .coordinator
            .try_promote("orders", 0, "failover-class", true, 5000)
            .unwrap();
        assert_eq!(new_ownership.owner_region, "region-b");
        assert_eq!(new_ownership.epoch, 2);

        let fence_err = fixture
            .coordinator
            .verify_write_epoch("orders", 0, 1)
            .unwrap_err();
        assert!(matches!(
            fence_err,
            OwnershipError::StaleWriterFenced { .. }
        ));
    }

    #[test]
    fn test_quarantine_superseded_records() {
        let fixture = setup_test_fixture(5000);
        let stale_record = RecordEnvelope::new(1, "failover-class", 1, 99, b"divergent".to_vec());
        fixture
            .coordinator
            .reconcile_divergent_suffix(vec![stale_record])
            .unwrap();

        assert_eq!(fixture.coordinator.quarantine().count(), 1);
    }
}
