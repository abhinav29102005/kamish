//! Operational Prometheus-Compatible Metrics Registry and Exporter for Kamish.
//!
//! Complies with Phase 6 / Workstream E:
//! - Thread-safe atomic counters and gauges.
//! - Bounded label cardinality (region, status, peer).
//! - Zero leakage of sensitive key material, credentials, or plaintext payloads.
//! - Complete coverage across produce, fetch, replication outbox, quarantine, ownership,
//!   consumer groups, storage retention, audit witness, sealed replication status, and
//!   in-region quorum consensus replication.
//! - Renders standard Prometheus text format 0.0.4 with `# HELP` and `# TYPE`.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct BrokerMetricsRegistry {
    // Produce metrics
    pub produce_success_total: AtomicU64,
    pub produce_error_total: AtomicU64,
    pub produce_bytes_total: AtomicU64,

    // Fetch metrics
    pub fetch_success_total: AtomicU64,
    pub fetch_error_total: AtomicU64,
    pub fetch_bytes_total: AtomicU64,

    // Replication metrics
    pub replication_in_success_total: AtomicU64,
    pub replication_in_conflict_total: AtomicU64,
    pub replication_out_success_total: AtomicU64,
    pub replication_out_failure_total: AtomicU64,

    // Ownership & Failover
    pub ownership_promotions_total: AtomicU64,
    pub lease_renewal_success_total: AtomicU64,
    pub lease_renewal_failure_total: AtomicU64,

    // Consumer metrics
    pub consumer_offset_commits_total: AtomicU64,
    pub consumer_offset_fetches_total: AtomicU64,
    pub consumer_rebalances_total: AtomicU64,

    // In-Region Quorum Consensus metrics
    pub quorum_appends_total: AtomicU64,
    pub quorum_appends_failed: AtomicU64,
}

impl BrokerMetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders all metrics in Prometheus text exposition format (v0.0.4).
    pub fn render_prometheus(&self, broker: &crate::BrokerServer) -> String {
        let region = &broker.region;
        let mut out = String::with_capacity(4096);

        // Produce metrics
        out.push_str(
            "# HELP kamish_produce_requests_total Total produce requests received by broker.\n",
        );
        out.push_str("# TYPE kamish_produce_requests_total counter\n");
        out.push_str(&format!(
            "kamish_produce_requests_total{{region=\"{}\",status=\"success\"}} {}\n",
            region,
            self.produce_success_total.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "kamish_produce_requests_total{{region=\"{}\",status=\"error\"}} {}\n",
            region,
            self.produce_error_total.load(Ordering::Relaxed)
        ));

        out.push_str(
            "# HELP kamish_produce_bytes_total Total payload bytes written to broker log.\n",
        );
        out.push_str("# TYPE kamish_produce_bytes_total counter\n");
        out.push_str(&format!(
            "kamish_produce_bytes_total{{region=\"{}\"}} {}\n",
            region,
            self.produce_bytes_total.load(Ordering::Relaxed)
        ));

        // Fetch metrics
        out.push_str(
            "# HELP kamish_fetch_requests_total Total fetch requests received by broker.\n",
        );
        out.push_str("# TYPE kamish_fetch_requests_total counter\n");
        out.push_str(&format!(
            "kamish_fetch_requests_total{{region=\"{}\",status=\"success\"}} {}\n",
            region,
            self.fetch_success_total.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "kamish_fetch_requests_total{{region=\"{}\",status=\"error\"}} {}\n",
            region,
            self.fetch_error_total.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP kamish_fetch_bytes_total Total payload bytes read from broker log.\n");
        out.push_str("# TYPE kamish_fetch_bytes_total counter\n");
        out.push_str(&format!(
            "kamish_fetch_bytes_total{{region=\"{}\"}} {}\n",
            region,
            self.fetch_bytes_total.load(Ordering::Relaxed)
        ));

        // Replication metrics
        out.push_str(
            "# HELP kamish_replication_in_records_total Records received from peer brokers.\n",
        );
        out.push_str("# TYPE kamish_replication_in_records_total counter\n");
        out.push_str(&format!(
            "kamish_replication_in_records_total{{region=\"{}\",status=\"success\"}} {}\n",
            region,
            self.replication_in_success_total.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "kamish_replication_in_records_total{{region=\"{}\",status=\"conflict\"}} {}\n",
            region,
            self.replication_in_conflict_total.load(Ordering::Relaxed)
        ));

        // Outbox queue metrics
        out.push_str(
            "# HELP kamish_outbox_queue_depth_bytes Current bytes buffered in destination outbox.\n",
        );
        out.push_str("# TYPE kamish_outbox_queue_depth_bytes gauge\n");
        let all_outboxes = broker.get_all_outboxes();
        for (target, outbox) in &all_outboxes {
            out.push_str(&format!(
                "kamish_outbox_queue_depth_bytes{{region=\"{}\",target=\"{}\"}} {}\n",
                region,
                target,
                outbox.queued_bytes()
            ));
        }

        out.push_str(
            "# HELP kamish_outbox_queue_depth_records Current records pending replication.\n",
        );
        out.push_str("# TYPE kamish_outbox_queue_depth_records gauge\n");
        for (target, outbox) in &all_outboxes {
            out.push_str(&format!(
                "kamish_outbox_queue_depth_records{{region=\"{}\",target=\"{}\"}} {}\n",
                region,
                target,
                outbox.queued_ranges()
            ));
        }

        // Quarantine metrics
        out.push_str("# HELP kamish_quarantine_records_total Quarantined divergent records isolated on disk.\n");
        out.push_str("# TYPE kamish_quarantine_records_total gauge\n");
        out.push_str(&format!(
            "kamish_quarantine_records_total{{region=\"{}\"}} {}\n",
            region,
            broker.quarantine.records().len()
        ));

        // Ownership metrics
        out.push_str(
            "# HELP kamish_ownership_promotions_total Total partition ownership failover promotions.\n",
        );
        out.push_str("# TYPE kamish_ownership_promotions_total counter\n");
        out.push_str(&format!(
            "kamish_ownership_promotions_total{{region=\"{}\"}} {}\n",
            region,
            self.ownership_promotions_total.load(Ordering::Relaxed)
        ));

        // Consumer Group metrics
        out.push_str(
            "# HELP kamish_consumer_groups_total Total consumer groups registered with broker.\n",
        );
        out.push_str("# TYPE kamish_consumer_groups_total gauge\n");
        let group_summaries = broker.consumer_groups.list_groups();
        out.push_str(&format!(
            "kamish_consumer_groups_total{{region=\"{}\"}} {}\n",
            region,
            group_summaries.len()
        ));

        out.push_str(
            "# HELP kamish_consumer_offset_commits_total Total offset commits accepted.\n",
        );
        out.push_str("# TYPE kamish_consumer_offset_commits_total counter\n");
        out.push_str(&format!(
            "kamish_consumer_offset_commits_total{{region=\"{}\"}} {}\n",
            region,
            self.consumer_offset_commits_total.load(Ordering::Relaxed)
        ));

        // Retention metrics
        let ret_metrics = broker.retention.metrics();
        out.push_str("# HELP kamish_retention_reclaimed_bytes_total Cumulative bytes reclaimed by retention cleaner.\n");
        out.push_str("# TYPE kamish_retention_reclaimed_bytes_total counter\n");
        out.push_str(&format!(
            "kamish_retention_reclaimed_bytes_total{{region=\"{}\"}} {}\n",
            region,
            ret_metrics.reclaimed_bytes_total.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP kamish_retention_reclaimed_records_total Cumulative records reclaimed by retention cleaner.\n");
        out.push_str("# TYPE kamish_retention_reclaimed_records_total counter\n");
        out.push_str(&format!(
            "kamish_retention_reclaimed_records_total{{region=\"{}\"}} {}\n",
            region,
            ret_metrics.reclaimed_records_total.load(Ordering::Relaxed)
        ));

        // Audit Witness publication metrics
        let wit_metrics = broker.witness_publisher.metrics();
        out.push_str("# HELP kamish_witness_pending_checkpoints Signed audit checkpoints awaiting external anchoring.\n");
        out.push_str("# TYPE kamish_witness_pending_checkpoints gauge\n");
        out.push_str(&format!(
            "kamish_witness_pending_checkpoints{{region=\"{}\"}} {}\n",
            region,
            wit_metrics.pending_count.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP kamish_witness_published_checkpoints_total Cumulative checkpoints anchored by witness authority.\n");
        out.push_str("# TYPE kamish_witness_published_checkpoints_total counter\n");
        out.push_str(&format!(
            "kamish_witness_published_checkpoints_total{{region=\"{}\"}} {}\n",
            region,
            wit_metrics.published_count.load(Ordering::Relaxed)
        ));

        // Sealed replication configuration gauge
        out.push_str("# HELP kamish_sealed_replication_configured Whether AEAD sealed replication key provider is active.\n");
        out.push_str("# TYPE kamish_sealed_replication_configured gauge\n");
        out.push_str(&format!(
            "kamish_sealed_replication_configured{{region=\"{}\"}} {}\n",
            region,
            if broker.key_provider.is_some() { 1 } else { 0 }
        ));

        // In-Region Quorum Consensus metrics
        let q_status = broker.quorum.status(broker);
        out.push_str(
            "# HELP kamish_quorum_configured Whether in-region replica group is configured.\n",
        );
        out.push_str("# TYPE kamish_quorum_configured gauge\n");
        out.push_str(&format!(
            "kamish_quorum_configured{{region=\"{}\"}} {}\n",
            region,
            if q_status.configured { 1 } else { 0 }
        ));

        out.push_str("# HELP kamish_quorum_is_leader Whether local node is the active in-region partition leader.\n");
        out.push_str("# TYPE kamish_quorum_is_leader gauge\n");
        out.push_str(&format!(
            "kamish_quorum_is_leader{{region=\"{}\"}} {}\n",
            region,
            if q_status.role == kamish_own::ReplicaRole::Leader {
                1
            } else {
                0
            }
        ));

        out.push_str("# HELP kamish_quorum_available Whether a majority of replicas are currently connected and available.\n");
        out.push_str("# TYPE kamish_quorum_available gauge\n");
        let connected_voters = 1 + q_status
            .peers
            .iter()
            .filter(|p| p.is_voter && p.connected)
            .count();
        let quorum_available = !q_status.configured || connected_voters >= q_status.quorum_size;
        out.push_str(&format!(
            "kamish_quorum_available{{region=\"{}\"}} {}\n",
            region,
            if quorum_available { 1 } else { 0 }
        ));

        out.push_str(
            "# HELP kamish_quorum_commit_index Committed watermark index per partition.\n",
        );
        out.push_str("# TYPE kamish_quorum_commit_index gauge\n");
        for (partition_key, idx) in &q_status.commit_indices {
            let parts: Vec<&str> = partition_key.split(':').collect();
            if parts.len() == 2 {
                out.push_str(&format!(
                    "kamish_quorum_commit_index{{region=\"{}\",topic=\"{}\",partition=\"{}\"}} {}\n",
                    region, parts[0], parts[1], idx
                ));
            }
        }

        out.push_str("# HELP kamish_quorum_available Whether a majority of replicas are currently connected and available.\n");
        out.push_str("# TYPE kamish_quorum_available gauge\n");
        let connected_voters = 1 + q_status
            .peers
            .iter()
            .filter(|p| p.is_voter && p.connected)
            .count();
        let quorum_available = !q_status.configured || connected_voters >= q_status.quorum_size;
        out.push_str(&format!(
            "kamish_quorum_available{{region=\"{}\"}} {}\n",
            region,
            if quorum_available { 1 } else { 0 }
        ));

        out.push_str(
            "# HELP kamish_quorum_commit_index Committed watermark index per partition.\n",
        );
        out.push_str("# TYPE kamish_quorum_commit_index gauge\n");
        for (partition_key, idx) in &q_status.commit_indices {
            let parts: Vec<&str> = partition_key.split(':').collect();
            if parts.len() == 2 {
                out.push_str(&format!(
                    "kamish_quorum_commit_index{{region=\"{}\",topic=\"{}\",partition=\"{}\"}} {}\n",
                    region, parts[0], parts[1], idx
                ));
            }
        }

        out.push_str(
            "# HELP kamish_quorum_voters_total Total voting nodes in the regional group.\n",
        );
        out.push_str("# TYPE kamish_quorum_voters_total gauge\n");
        out.push_str(&format!(
            "kamish_quorum_voters_total{{region=\"{}\"}} {}\n",
            region, q_status.total_voters
        ));

        out.push_str(
            "# HELP kamish_quorum_size Strict majority quorum size required for durable ack.\n",
        );
        out.push_str("# TYPE kamish_quorum_size gauge\n");
        out.push_str(&format!(
            "kamish_quorum_size{{region=\"{}\"}} {}\n",
            region, q_status.quorum_size
        ));

        out.push_str("# HELP kamish_quorum_appends_total Total produce appends requiring in-region quorum.\n");
        out.push_str("# TYPE kamish_quorum_appends_total counter\n");
        out.push_str(&format!(
            "kamish_quorum_appends_total{{region=\"{}\"}} {}\n",
            region,
            broker.quorum.quorum_appends_total.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP kamish_quorum_appends_failed_total Total in-region quorum appends failed or timed out.\n");
        out.push_str("# TYPE kamish_quorum_appends_failed_total counter\n");
        out.push_str(&format!(
            "kamish_quorum_appends_failed_total{{region=\"{}\"}} {}\n",
            region,
            broker.quorum.quorum_appends_failed.load(Ordering::Relaxed)
        ));

        out
    }
}
