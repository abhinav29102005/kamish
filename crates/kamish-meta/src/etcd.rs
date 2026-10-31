//! Authoritative etcd-backed MetadataAuthority implementation
//!
//! Complies with:
//! - Master Plan Phase 1 (Item 1.2)
//! - ADR-0015 / ADR-0017
//! - `docs/CONTRACT.md` (§2.1, §2.2, §2.3.1, §3.B, §3.C)
//! - Pure-Rust TLS (mTLS with rustls / ring)
//! - Independent epoch key (never lease-attached, never regresses)
//! - Single atomic compare_and_swap_epoch Txn
//! - Local monotonic lease clock validation with drift/transit deduction
//! - Non-watch based revocation and fail-closed quorum loss

use crate::{
    AuditCheckpoint, CheckpointReceipt, DeploymentMode, LeaseGrant, LocalLease, MetaError,
    MetadataAuthority, MonotonicClock, PartitionOwnership, RevocationAck, SourceIdentityGrant,
    StreamId, SystemMonotonicClock,
};
use chrono::Utc;
use etcd_client::{
    Certificate, Client, Compare, CompareOp, ConnectOptions, Identity, PutOptions, TlsOptions, Txn,
    TxnOp,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnerRecord {
    pub owner_region: String,
    pub epoch: u64,
    pub lease_id: i64,
    pub granted_at_ms: i64,
    pub duration_ms: i64,
}

/// Etcd-backed Metadata Authority providing strongly-consistent multi-region consensus.
pub struct EtcdMetadataAuthority {
    endpoints: Vec<String>,
    tls_options: Option<TlsOptions>,
    client: Arc<TokioMutex<Option<Client>>>,
    clock: Arc<dyn MonotonicClock>,
    max_clock_drift_ms: u64,
    transit_delay_allowance_ms: u64,
    runtime: Arc<tokio::runtime::Runtime>,
    local_leases: Arc<std::sync::RwLock<HashMap<(String, u32), LocalLease>>>,
}

impl EtcdMetadataAuthority {
    /// Creates a new `EtcdMetadataAuthority` connecting to the specified endpoints.
    pub fn new<P: AsRef<Path>>(
        endpoints: Vec<String>,
        ca_cert_path: Option<P>,
        client_cert_path: Option<P>,
        client_key_path: Option<P>,
        clock: Option<Arc<dyn MonotonicClock>>,
    ) -> Result<Self, MetaError> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| MetaError::AuthorityError(format!("Failed to build runtime: {e}")))?,
        );

        let tls_options = match (ca_cert_path, client_cert_path, client_key_path) {
            (Some(ca_p), Some(cert_p), Some(key_p)) => {
                let ca_pem = fs::read(ca_p).map_err(|e| {
                    MetaError::AuthorityError(format!("Failed to read CA cert: {e}"))
                })?;
                let cert_pem = fs::read(cert_p).map_err(|e| {
                    MetaError::AuthorityError(format!("Failed to read client cert: {e}"))
                })?;
                let key_pem = fs::read(key_p).map_err(|e| {
                    MetaError::AuthorityError(format!("Failed to read client key: {e}"))
                })?;

                let tls = TlsOptions::new()
                    .ca_certificate(Certificate::from_pem(ca_pem))
                    .identity(Identity::from_pem(cert_pem, key_pem));
                Some(tls)
            }
            _ => None,
        };

        let monotonic_clock = clock.unwrap_or_else(|| Arc::new(SystemMonotonicClock::new()));

        let authority = Self {
            endpoints,
            tls_options,
            client: Arc::new(TokioMutex::new(None)),
            clock: monotonic_clock,
            max_clock_drift_ms: 200,
            transit_delay_allowance_ms: 100,
            runtime,
            local_leases: Arc::new(std::sync::RwLock::new(HashMap::new())),
        };

        // Eagerly connect
        authority.block_on(authority.ensure_connected())?;

        Ok(authority)
    }

    fn block_on<F: std::future::Future>(&self, f: F) -> F::Output {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(f)),
            Err(_) => self.runtime.block_on(f),
        }
    }

    async fn ensure_connected(&self) -> Result<Client, MetaError> {
        let mut guard = self.client.lock().await;
        if let Some(ref client) = *guard {
            return Ok(client.clone());
        }

        let mut options = ConnectOptions::new()
            .with_connect_timeout(Duration::from_millis(1500))
            .with_timeout(Duration::from_secs(4));
        if let Some(ref tls) = self.tls_options {
            options = options.with_tls(tls.clone());
        }

        let client = Client::connect(self.endpoints.clone(), Some(options))
            .await
            .map_err(|e| {
                MetaError::AuthorityError(format!("Failed to connect to etcd endpoints: {e}"))
            })?;

        *guard = Some(client.clone());
        Ok(client)
    }

    /// Resets the internal client channel (useful when testing failover/reconnection).
    pub fn reset_connection(&self) {
        self.block_on(async {
            let mut guard = self.client.lock().await;
            *guard = None;
        });
    }

    fn epoch_key(stream: &StreamId) -> String {
        format!("kamish/streams/{}/{}/epoch", stream.topic, stream.partition)
    }

    fn owner_key(stream: &StreamId) -> String {
        format!("kamish/streams/{}/{}/owner", stream.topic, stream.partition)
    }

    fn floor_key(cluster_id: &str) -> String {
        format!("kamish/clusters/{cluster_id}/policy/floor")
    }

    fn source_gen_key(cluster_id: &str) -> String {
        format!("kamish/clusters/{cluster_id}/source_generation")
    }

    fn audit_key(stream: &StreamId) -> String {
        format!("kamish/audit/streams/{}/{}", stream.topic, stream.partition)
    }

    /// Returns the cached local lease for partition write admission checks.
    pub fn get_local_lease(&self, stream: &StreamId) -> Option<LocalLease> {
        let key = (stream.topic.clone(), stream.partition);
        self.local_leases.read().unwrap().get(&key).cloned()
    }

    /// Queries the authoritative ownership record from consensus directly without checking local lease cache.
    pub fn get_ownership_consensus(&self, stream: &StreamId) -> Option<PartitionOwnership> {
        self.block_on(async {
            let client = self.ensure_connected().await.ok()?;
            let mut kv_client = client.kv_client();
            let resp = kv_client.get(Self::owner_key(stream), None).await.ok()?;
            let kv = resp.kvs().first()?;
            let record: OwnerRecord = serde_json::from_slice(kv.value()).ok()?;
            let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
            Some(PartitionOwnership {
                topic: stream.topic.clone(),
                partition: stream.partition,
                owner_region: record.owner_region,
                epoch: record.epoch,
                lease_expiry_ns: now_ns + (record.duration_ms * 1_000_000),
            })
        })
    }

    fn try_compare_and_swap_epoch_once(
        &self,
        stream: &StreamId,
        expected_epoch: u64,
        new_epoch: u64,
        new_owner_region: &str,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let epoch_k = Self::epoch_key(stream);
            let owner_k = Self::owner_key(stream);

            // 1. Grant etcd lease with duration
            let ttl_sec = std::cmp::max(1, lease_duration_ms / 1000);
            let mut lease_client = client.lease_client();
            let grant_res = lease_client
                .grant(ttl_sec, None)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("etcd lease grant error: {e}")))?;
            let lease_id = grant_res.id();

            // Record local monotonic receipt timestamp immediately after grant
            let receipt_monotonic_ms = self.clock.now_monotonic_ms();

            let record = OwnerRecord {
                owner_region: new_owner_region.to_string(),
                epoch: new_epoch,
                lease_id,
                granted_at_ms: Utc::now().timestamp_millis(),
                duration_ms: lease_duration_ms,
            };
            let record_json = serde_json::to_string(&record)
                .map_err(|e| MetaError::AuthorityError(format!("JSON serialization error: {e}")))?;

            // 2. Build atomic CAS Txn
            let compare = if expected_epoch == 0 {
                Compare::version(epoch_k.clone(), CompareOp::Equal, 0)
            } else {
                Compare::value(
                    epoch_k.clone(),
                    CompareOp::Equal,
                    expected_epoch.to_string(),
                )
            };

            let put_owner = TxnOp::put(
                owner_k.clone(),
                record_json,
                Some(PutOptions::new().with_lease(lease_id)),
            );
            let put_epoch = TxnOp::put(epoch_k.clone(), new_epoch.to_string(), None);

            let txn = Txn::new()
                .when([compare])
                .and_then([put_owner, put_epoch])
                .or_else([TxnOp::get(epoch_k.clone(), None)]);

            let mut kv_client = client.kv_client();
            let txn_res = kv_client
                .txn(txn)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("etcd Txn CAS error: {e}")))?;

            if !txn_res.succeeded() {
                // Determine actual committed epoch from failure read
                let actual_epoch = match txn_res.op_responses().first() {
                    Some(etcd_client::TxnOpResponse::Get(get_resp)) => {
                        if let Some(kv) = get_resp.kvs().first() {
                            std::str::from_utf8(kv.value())
                                .ok()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    }
                    _ => 0,
                };

                // Ambiguous commit handling: if actual_epoch is already new_epoch,
                // check if the existing committed owner is this claimant (idempotent retry).
                if actual_epoch == new_epoch {
                    let matching_owner = kv_client
                        .get(owner_k.clone(), None)
                        .await
                        .ok()
                        .and_then(|r| r.kvs().first().cloned())
                        .and_then(|kv| serde_json::from_slice::<OwnerRecord>(kv.value()).ok())
                        .is_some_and(|rec| rec.owner_region == new_owner_region);

                    if matching_owner {
                        let _ = lease_client.revoke(lease_id).await;
                        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
                        return Ok(PartitionOwnership {
                            topic: stream.topic.clone(),
                            partition: stream.partition,
                            owner_region: new_owner_region.to_string(),
                            epoch: new_epoch,
                            lease_expiry_ns: now_ns + (lease_duration_ms * 1_000_000),
                        });
                    }
                }

                // Losing claimant cleanup: revoke the redundant lease we granted
                let _ = lease_client.revoke(lease_id).await;

                return Err(MetaError::StaleEpoch {
                    current: actual_epoch,
                    proposed: expected_epoch,
                });
            }

            // 3. CAS succeeded: register LocalLease with monotonic clock rules
            let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
            let expiry_ns = now_ns + (lease_duration_ms * 1_000_000);
            let grant = LeaseGrant {
                lease_id: lease_id as u64,
                stream: stream.clone(),
                holder_region: new_owner_region.to_string(),
                granted_at_ns: now_ns,
                expiry_ns,
                epoch: new_epoch,
            };

            let local_lease = LocalLease::from_grant(
                &grant,
                receipt_monotonic_ms,
                self.max_clock_drift_ms,
                self.transit_delay_allowance_ms,
            );

            let stream_key = (stream.topic.clone(), stream.partition);
            self.local_leases
                .write()
                .unwrap()
                .insert(stream_key, local_lease);

            Ok(PartitionOwnership {
                topic: stream.topic.clone(),
                partition: stream.partition,
                owner_region: new_owner_region.to_string(),
                epoch: new_epoch,
                lease_expiry_ns: expiry_ns,
            })
        })
    }
}

impl MetadataAuthority for EtcdMetadataAuthority {
    fn deployment_mode(&self) -> DeploymentMode {
        DeploymentMode::MultiRegion
    }

    fn compare_and_swap_epoch(
        &self,
        stream: &StreamId,
        expected_epoch: u64,
        new_epoch: u64,
        new_owner_region: &str,
        lease_duration_ms: i64,
    ) -> Result<PartitionOwnership, MetaError> {
        if new_epoch <= expected_epoch && expected_epoch != 0 {
            return Err(MetaError::StaleEpoch {
                current: expected_epoch,
                proposed: new_epoch,
            });
        }

        let max_attempts = 4;
        let mut last_err =
            MetaError::AuthorityError("Operation failed without attempts".to_string());

        for attempt in 1..=max_attempts {
            // Ambiguous commit check before re-attempting:
            // If the key is already committed to new_epoch and this region, it already succeeded!
            if attempt > 1
                && let Some(existing) = self
                    .get_ownership_consensus(stream)
                    .filter(|ex| ex.epoch == new_epoch && ex.owner_region == new_owner_region)
            {
                return Ok(existing);
            }

            match self.try_compare_and_swap_epoch_once(
                stream,
                expected_epoch,
                new_epoch,
                new_owner_region,
                lease_duration_ms,
            ) {
                Ok(ownership) => return Ok(ownership),
                Err(MetaError::StaleEpoch { current, proposed }) => {
                    // Check if current epoch matches our target (ambiguous commit replay)
                    if current == new_epoch
                        && let Some(existing) = self.get_ownership_consensus(stream).filter(|ex| {
                            ex.epoch == new_epoch && ex.owner_region == new_owner_region
                        })
                    {
                        return Ok(existing);
                    }
                    return Err(MetaError::StaleEpoch { current, proposed });
                }
                Err(err) => {
                    last_err = err;
                    if attempt < max_attempts {
                        // Allow Tower balanced channel to rotate to healthy endpoints
                        // before destroying channel state
                        if attempt >= 2 {
                            self.reset_connection();
                        }
                        std::thread::sleep(Duration::from_millis(300 * attempt as u64));
                    }
                }
            }
        }

        Err(last_err)
    }

    fn grant_lease(
        &self,
        stream: &StreamId,
        holder_region: &str,
        duration_ms: i64,
    ) -> Result<LeaseGrant, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut lease_client = client.lease_client();
            let ttl_sec = std::cmp::max(1, duration_ms / 1000);

            let grant = lease_client
                .grant(ttl_sec, None)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("etcd grant_lease error: {e}")))?;
            let lease_id = grant.id() as u64;

            let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
            let expiry_ns = now_ns + (duration_ms * 1_000_000);

            let receipt_ms = self.clock.now_monotonic_ms();
            let lease_grant = LeaseGrant {
                lease_id,
                stream: stream.clone(),
                holder_region: holder_region.to_string(),
                granted_at_ns: now_ns,
                expiry_ns,
                epoch: 1,
            };

            let local_lease = LocalLease::from_grant(
                &lease_grant,
                receipt_ms,
                self.max_clock_drift_ms,
                self.transit_delay_allowance_ms,
            );

            self.local_leases
                .write()
                .unwrap()
                .insert((stream.topic.clone(), stream.partition), local_lease);

            Ok(lease_grant)
        })
    }

    fn renew_lease(
        &self,
        stream: &StreamId,
        lease_id: u64,
        _duration_ms: i64,
    ) -> Result<LeaseGrant, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut lease_client = client.lease_client();

            let (mut keeper, mut stream_msg) = lease_client
                .keep_alive(lease_id as i64)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("keep_alive init error: {e}")))?;

            keeper
                .keep_alive()
                .await
                .map_err(|e| MetaError::AuthorityError(format!("keep_alive ping error: {e}")))?;

            let resp = stream_msg.message().await.map_err(|e| {
                MetaError::AuthorityError(format!("keep_alive response error: {e}"))
            })?;

            let ttl_remaining = match resp {
                Some(r) => r.ttl(),
                None => {
                    return Err(MetaError::LeaseNotFound(lease_id));
                }
            };

            let receipt_ms = self.clock.now_monotonic_ms();
            let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
            let expiry_ns = now_ns + ((ttl_remaining * 1000) * 1_000_000);

            let lease_grant = LeaseGrant {
                lease_id,
                stream: stream.clone(),
                holder_region: "renewed-region".to_string(),
                granted_at_ns: now_ns,
                expiry_ns,
                epoch: 1,
            };

            let local_lease = LocalLease::from_grant(
                &lease_grant,
                receipt_ms,
                self.max_clock_drift_ms,
                self.transit_delay_allowance_ms,
            );

            self.local_leases
                .write()
                .unwrap()
                .insert((stream.topic.clone(), stream.partition), local_lease);

            Ok(lease_grant)
        })
    }

    fn revoke_lease(&self, stream: &StreamId, lease_id: u64) -> Result<RevocationAck, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut lease_client = client.lease_client();

            let _ = lease_client.revoke(lease_id as i64).await;

            let owner_k = Self::owner_key(stream);
            let mut kv_client = client.kv_client();
            let _ = kv_client.delete(owner_k, None).await;

            self.local_leases
                .write()
                .unwrap()
                .remove(&(stream.topic.clone(), stream.partition));

            let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
            Ok(RevocationAck {
                lease_id,
                stream: stream.clone(),
                revoked_at_ns: now_ns,
                effective_boundary_ns: now_ns,
            })
        })
    }

    fn get_policy_floor(&self, cluster_id: &str) -> Result<u64, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut kv_client = client.kv_client();
            let resp = kv_client
                .get(Self::floor_key(cluster_id), None)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("etcd get floor error: {e}")))?;

            if let Some(kv) = resp.kvs().first() {
                let s = std::str::from_utf8(kv.value())
                    .map_err(|e| MetaError::AuthorityError(format!("UTF8 error: {e}")))?;
                s.parse::<u64>()
                    .map_err(|e| MetaError::AuthorityError(format!("Parse u64 error: {e}")))
            } else {
                Ok(0)
            }
        })
    }

    fn advance_policy_floor(&self, cluster_id: &str, new_floor: u64) -> Result<u64, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut kv_client = client.kv_client();
            let key = Self::floor_key(cluster_id);

            let get_resp = kv_client
                .get(key.clone(), None)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("get floor error: {e}")))?;

            let (current_floor, version) = if let Some(kv) = get_resp.kvs().first() {
                let val_str = std::str::from_utf8(kv.value()).unwrap_or("0");
                let cur = val_str.parse::<u64>().unwrap_or(0);
                (cur, kv.version())
            } else {
                (0, 0)
            };

            if new_floor <= current_floor {
                return Err(MetaError::PolicyFloorRegression {
                    current: current_floor,
                    proposed: new_floor,
                });
            }

            let compare = Compare::version(key.clone(), CompareOp::Equal, version);
            let put = TxnOp::put(key, new_floor.to_string(), None);

            let txn = Txn::new().when([compare]).and_then([put]).or_else([]);
            let res = kv_client
                .txn(txn)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("Txn advance floor error: {e}")))?;

            if !res.succeeded() {
                return Err(MetaError::AuthorityError(
                    "Concurrent advance_policy_floor CAS conflict".to_string(),
                ));
            }

            Ok(new_floor)
        })
    }

    fn issue_source_identity(
        &self,
        cluster_id: &str,
        node_id: &str,
    ) -> Result<SourceIdentityGrant, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut kv_client = client.kv_client();
            let key = Self::source_gen_key(cluster_id);

            for _ in 0..10 {
                let get_resp = kv_client
                    .get(key.clone(), None)
                    .await
                    .map_err(|e| MetaError::AuthorityError(format!("get gen error: {e}")))?;

                let (current_gen, version) = if let Some(kv) = get_resp.kvs().first() {
                    let val_str = std::str::from_utf8(kv.value()).unwrap_or("0");
                    let cur = val_str.parse::<u64>().unwrap_or(0);
                    (cur, kv.version())
                } else {
                    (0, 0)
                };

                let next_gen = current_gen + 1;
                let compare = Compare::version(key.clone(), CompareOp::Equal, version);
                let put = TxnOp::put(key.clone(), next_gen.to_string(), None);

                let txn = Txn::new().when([compare]).and_then([put]).or_else([]);
                let txn_res = kv_client
                    .txn(txn)
                    .await
                    .map_err(|e| MetaError::AuthorityError(format!("Txn gen error: {e}")))?;

                if txn_res.succeeded() {
                    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
                    return Ok(SourceIdentityGrant {
                        cluster_id: cluster_id.to_string(),
                        node_id: node_id.to_string(),
                        generation: next_gen,
                        issued_at_ns: now_ns,
                    });
                }
            }

            Err(MetaError::AuthorityError(
                "Failed to increment source generation after 10 attempts".to_string(),
            ))
        })
    }

    fn commit_audit_checkpoint(
        &self,
        checkpoint: AuditCheckpoint,
    ) -> Result<CheckpointReceipt, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut kv_client = client.kv_client();
            let key = Self::audit_key(&checkpoint.stream);

            let json = serde_json::to_string(&checkpoint).map_err(|e| {
                MetaError::AuthorityError(format!("Checkpoint serialization error: {e}"))
            })?;

            let put_resp = kv_client
                .put(key, json, None)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("put checkpoint error: {e}")))?;

            let rev = put_resp.header().map(|h| h.revision() as u64).unwrap_or(1);
            let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);

            Ok(CheckpointReceipt {
                stream: checkpoint.stream,
                checkpoint_sequence: checkpoint.sequence,
                committed_revision: rev,
                anchored_at_ns: now_ns,
            })
        })
    }

    fn read_audit_checkpoint(
        &self,
        stream: &StreamId,
    ) -> Result<Option<AuditCheckpoint>, MetaError> {
        self.block_on(async {
            let client = self.ensure_connected().await?;
            let mut kv_client = client.kv_client();
            let key = Self::audit_key(stream);

            let resp = kv_client
                .get(key, None)
                .await
                .map_err(|e| MetaError::AuthorityError(format!("get checkpoint error: {e}")))?;

            if let Some(kv) = resp.kvs().first() {
                let checkpoint: AuditCheckpoint =
                    serde_json::from_slice(kv.value()).map_err(|e| {
                        MetaError::AuthorityError(format!("Checkpoint parse error: {e}"))
                    })?;
                Ok(Some(checkpoint))
            } else {
                Ok(None)
            }
        })
    }

    fn get_ownership(&self, topic: &str, partition: u32) -> Option<PartitionOwnership> {
        let stream = StreamId::new(topic, partition);
        let owner_k = Self::owner_key(&stream);

        self.block_on(async {
            let client = self.ensure_connected().await.ok()?;
            let mut kv_client = client.kv_client();

            let resp = kv_client.get(owner_k, None).await.ok()?;
            let kv = resp.kvs().first()?;

            let record: OwnerRecord = serde_json::from_slice(kv.value()).ok()?;

            let local_lease = self.get_local_lease(&stream);
            let is_valid = match local_lease {
                Some(ref lease) => lease.is_valid(self.clock.now_monotonic_ms()),
                None => true,
            };

            if !is_valid {
                return None;
            }

            let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
            Some(PartitionOwnership {
                topic: topic.to_string(),
                partition,
                owner_region: record.owner_region,
                epoch: record.epoch,
                lease_expiry_ns: now_ns + (record.duration_ms * 1_000_000),
            })
        })
    }
}
