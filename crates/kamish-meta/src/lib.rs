//! Metadata Entity-Relationship Model for Kamish.
//!
//! Complies with `project.md`:
//! - §2.5: Metadata Entity-Relationship Model (Region, Topic, Partition, Ownership, Replica, OutboxStream).
//! - §2.2: Global Metadata Quorum (holds ownership, epochs, policy versions; holds NO payload).
//! - INV-7: No two regions commit writes for the same partition and epoch.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

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

/// Partition descriptor entity (§2.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionMeta {
    pub topic: String,
    pub partition_id: u32,
    pub class_id: String,
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

    /// Registers a new topic with defined data class and partitions.
    pub fn create_topic(&self, topic_id: impl Into<String>, class_id: impl Into<String>, partition_count: u32) -> TopicMeta {
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

    /// Assigns initial ownership for a partition.
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

    /// Compares and swaps ownership on failover promotion (§3.4.4, INV-7).
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
                })
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
        inner.ownerships.get(&(topic.to_string(), partition)).cloned()
    }

    pub fn update_watermark(&self, topic: &str, partition: u32, region: &str, watermark: u64) {
        let mut inner = self.inner.write().unwrap();
        let key = (topic.to_string(), partition, region.to_string());
        if let Some(r) = inner.replicas.get_mut(&key) {
            if watermark > r.durable_watermark {
                r.durable_watermark = watermark;
            }
        } else {
            inner.replicas.insert(key, ReplicaState {
                topic: topic.to_string(),
                partition,
                region_id: region.to_string(),
                dest_mode: "CLEARTEXT".to_string(),
                durable_watermark: watermark,
            });
        }
    }

    pub fn get_watermark(&self, topic: &str, partition: u32, region: &str) -> u64 {
        let inner = self.inner.read().unwrap();
        inner.replicas
            .get(&(topic.to_string(), partition, region.to_string()))
            .map(|r| r.durable_watermark)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ownership_and_cas_promotion() {
        let meta = MetadataStore::new();
        meta.create_topic("payments", "EU-restricted", 4);

        let o1 = meta.set_ownership("payments", 0, "region-a", 1, 5000).unwrap();
        assert_eq!(o1.epoch, 1);
        assert_eq!(o1.owner_region, "region-a");

        // Failover promotion CAS (epoch increases strictly by 1)
        let o2 = meta.compare_and_swap_owner("payments", 0, 1, "region-b", 5000).unwrap();
        assert_eq!(o2.epoch, 2);
        assert_eq!(o2.owner_region, "region-b");

        // Stale CAS attempt fails (INV-7)
        let err = meta.compare_and_swap_owner("payments", 0, 1, "region-c", 5000).unwrap_err();
        assert!(matches!(err, MetaError::StaleEpoch { current: 2, proposed: 1 }));
    }

    #[test]
    fn test_watermark_tracking() {
        let meta = MetadataStore::new();
        meta.update_watermark("logs", 0, "region-b", 100);
        assert_eq!(meta.get_watermark("logs", 0, "region-b"), 100);

        meta.update_watermark("logs", 0, "region-b", 150);
        assert_eq!(meta.get_watermark("logs", 0, "region-b"), 150);

        // Does not regress watermark
        meta.update_watermark("logs", 0, "region-b", 80);
        assert_eq!(meta.get_watermark("logs", 0, "region-b"), 150);
    }
}

impl MetadataStore {
    pub fn record_outbox_stream(&self, meta: OutboxStreamMeta) {
        let mut inner = self.inner.write().unwrap();
        let key = (meta.topic.clone(), meta.partition, meta.dest_region.clone());
        inner.outbox_streams.insert(key, meta);
    }

    pub fn get_outbox_stream(&self, topic: &str, partition: u32, dest: &str) -> Option<OutboxStreamMeta> {
        let inner = self.inner.read().unwrap();
        inner.outbox_streams.get(&(topic.to_string(), partition, dest.to_string())).cloned()
    }
}
