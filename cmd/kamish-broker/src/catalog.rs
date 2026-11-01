use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Metadata defining ownership, allowed classes, and access boundaries for a cataloged topic (W10 / W13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicMetadata {
    pub topic: String,
    pub owner_tenant: String,
    pub partitions: u32,
    pub allowed_classes: HashSet<String>,

    #[serde(default)]
    pub authorized_producers: HashSet<String>,

    #[serde(default)]
    pub authorized_consumers: HashSet<String>,

    #[serde(default)]
    pub allowed_consumer_regions: HashSet<String>,
}

impl TopicMetadata {
    pub fn new(
        topic: impl Into<String>,
        owner_tenant: impl Into<String>,
        partitions: u32,
        allowed_classes: HashSet<String>,
    ) -> Self {
        Self {
            topic: topic.into(),
            owner_tenant: owner_tenant.into(),
            partitions,
            allowed_classes,
            authorized_producers: HashSet::new(),
            authorized_consumers: HashSet::new(),
            allowed_consumer_regions: HashSet::new(),
        }
    }

    pub fn with_consumer_regions(mut self, regions: HashSet<String>) -> Self {
        self.allowed_consumer_regions = regions;
        self
    }

    pub fn with_producers(mut self, producers: HashSet<String>) -> Self {
        self.authorized_producers = producers;
        self
    }

    pub fn with_consumers(mut self, consumers: HashSet<String>) -> Self {
        self.authorized_consumers = consumers;
        self
    }
}

/// Authorized topic and tenant catalog preventing cross-tenant access and unauthorized class selection (W10 / W13).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantCatalog {
    pub topics: HashMap<String, TopicMetadata>,
}

impl TenantCatalog {
    pub fn new() -> Self {
        Self {
            topics: HashMap::new(),
        }
    }

    pub fn register_topic(&mut self, metadata: TopicMetadata) {
        self.topics.insert(metadata.topic.clone(), metadata);
    }

    pub fn get_topic(&self, topic: &str) -> Option<&TopicMetadata> {
        self.topics.get(topic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_topic_registration() {
        let mut catalog = TenantCatalog::new();
        let mut classes = HashSet::new();
        classes.insert("fintech-transfers".into());

        let meta = TopicMetadata::new("payments", "tenant-fintech", 4, classes)
            .with_consumer_regions(["EU_WEST"].into_iter().map(String::from).collect());

        catalog.register_topic(meta);

        let topic = catalog.get_topic("payments").unwrap();
        assert_eq!(topic.owner_tenant, "tenant-fintech");
        assert!(topic.allowed_classes.contains("fintech-transfers"));
        assert!(topic.allowed_consumer_regions.contains("EU_WEST"));
        assert!(!topic.allowed_consumer_regions.contains("US_EAST"));
    }
}
