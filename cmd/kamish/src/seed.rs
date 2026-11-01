use chrono::Utc;
use kamish_broker::{BrokerRequest, BrokerServer, ProduceRequest};
use kamish_meta::{AuditCheckpoint, RegionId, ResidencyPolicy, StreamId};
use serde_json::json;
use std::sync::Arc;

pub fn seed_development_data(broker: &Arc<BrokerServer>) {
    // Only seed if log for financial-transactions:0 is empty
    let already_seeded = broker
        .get_or_create_log("financial-transactions", 0, 1)
        .map(|l| l.high_watermark() > 0)
        .unwrap_or(false);

    if already_seeded {
        tracing::info!("Broker partition logs already contain data; skipping initial seed.");
        return;
    }

    println!("[kamish dev] Seeding realistic enterprise multi-partition event streams and audit trails...");

    let mut pid: u64 = 100_000;

    let mut produce = |topic: &str, partition: u32, class_id: &str, policy: Option<ResidencyPolicy>, val: serde_json::Value| {
        pid += 1;
        let payload = serde_json::to_vec(&val).unwrap_or_default();
        let req = ProduceRequest {
            topic: topic.to_string(),
            partition,
            producer_id: pid,
            producer_epoch: 1,
            sequence_base: 0,
            class_id: class_id.to_string(),
            payload,
            residency_rule: policy,
        };
        let _ = broker.dispatch(BrokerRequest::Produce(req));
    };

    // =========================================================================
    // 1. Topic: financial-transactions (Tenant: tenant-sovereign)
    // =========================================================================
    // Partition 0
    produce(
        "financial-transactions",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "message_standard": "ISO20022_pacs_008_001_08",
            "instruction_id": "SEPA-INST-2026-EU-98124501",
            "end_to_end_id": "E2E-2026-EU-991823",
            "clearing_system": "TIPS_ECB_FRANKFURT",
            "settlement_currency": "EUR",
            "settlement_amount": 125400.00,
            "settlement_date": "2026-11-01",
            "debtor": {
                "name": "Siemens Energy AG",
                "iban": "DE89370400440532013000",
                "bic": "DBEUMM21XXX",
                "country": "DE"
            },
            "creditor": {
                "name": "Schneider Electric SE",
                "iban": "FR7630006000011234567890189",
                "bic": "BNPAFRPPXXX",
                "country": "FR"
            },
            "aml_compliance": {
                "screening_status": "PASSED",
                "sanctions_check": "CLEARED",
                "pep_match": false,
                "regulatory_framework": "EU_AML_DIRECTIVE_6"
            },
            "sovereignty_rule": "EU_STRICT_LOCAL_NON_REPLICABLE"
        }),
    );

    produce(
        "financial-transactions",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "message_standard": "TARGET2_RTGS_WHOLESALE",
            "settlement_reference": "T2-RTGS-WHOLESALE-20261101-004",
            "debtor_central_bank": "DE_BUNDESBANK_001",
            "creditor_central_bank": "FR_BANQUE_DE_FRANCE_002",
            "settlement_amount": 18500000.00,
            "currency": "EUR",
            "clearing_cycle": "09:30:00.000_UTC",
            "collateral_status": "ECB_TIER1_GOV_BONDS_RESERVED",
            "settlement_finality_directive": "DIRECTIVE_98_26_EC_CONFIRMED"
        }),
    );

    produce(
        "financial-transactions",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "message_standard": "SWIFT_GPI_CONFIRMATION",
            "uetr": "c49b8821-2f3b-482a-8991-89d20c581fa2",
            "settlement_amount": 840000.00,
            "currency": "USD",
            "ordering_customer": "Deutsche Bank AG Frankfurt Treasury Desk",
            "beneficiary_institution": "Bank of New York Mellon, NYC",
            "charge_details": "OUR",
            "timestamp": "2026-11-01T08:12:44.192Z",
            "egress_boundary": "RESTRICTED_REPLICATION_AUTHORIZED"
        }),
    );

    // Partition 1
    produce(
        "financial-transactions",
        1,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "message_standard": "ECB_EUROSYSTEM_STANDING_FACILITIES",
            "facility_id": "ECB-DF-2026-1101-7891",
            "facility_type": "OVERNIGHT_DEPOSIT_FACILITY",
            "deposit_amount": 45000000.00,
            "currency": "EUR",
            "interest_rate_pct": 3.25,
            "booking_center": "Frankfurt_Am_Main",
            "settlement_status": "COMMITTED"
        }),
    );

    produce(
        "financial-transactions",
        1,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "message_standard": "FX_SPOT_CONFIRMATION",
            "trade_id": "FX-SPOT-EURCHF-891044",
            "currency_pair": "EUR/CHF",
            "base_amount": 5000000.00,
            "base_currency": "EUR",
            "quote_amount": 4710500.00,
            "quote_currency": "CHF",
            "executed_rate": 0.94210,
            "clearing_venue": "SIX_SIS_SWITZERLAND",
            "regulatory_reporting": "EMIR_ARTICLE_9_COMPLIANT"
        }),
    );

    // Partition 2
    produce(
        "financial-transactions",
        2,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "message_standard": "BASEL_III_LIQUIDITY_COVERAGE",
            "transfer_id": "LCR-HQLA-20261101-0941",
            "asset_class": "LEVEL_1_CENTRAL_BANK_RESERVES",
            "nominal_value": 75000000.00,
            "currency": "EUR",
            "counterparty_lei": "5493006MHB84DD0ZWV18",
            "prudential_authority": "ECB_SSM_JOINT_SUPERVISORY_TEAM"
        }),
    );

    produce(
        "financial-transactions",
        2,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "message_standard": "MIFID_II_RTS25_TIME_PROOF",
            "report_id": "MIFID-RTS25-20261101-884",
            "execution_timestamp_utc": "2026-11-01T09:42:15.891245192Z",
            "time_source": "PTP_IEEE_1588_UTC_TRACEABLE",
            "clock_divergence_ns": 14,
            "trading_venue_mic": "XFRA",
            "algo_execution_flag": true
        }),
    );

    // =========================================================================
    // 2. Topic: sovereign-stream (Tenant: tenant-sovereign)
    // =========================================================================
    // Partition 0
    produce(
        "sovereign-stream",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "telemetry_type": "HARDWARE_SECURITY_MODULE_TPM_QUOTE",
            "quote_id": "TPM2-QUOTE-FRA4-20261101-01",
            "node_id": "fra-sovereign-node-01",
            "datacenter": "Frankfurt_DC4_Sovereign_Enclave",
            "security_certification": "COMMON_CRITERIA_EAL5_PLUS",
            "pcr_digest_sha256": "7b51e04138a42e5788db1261d71fa904bc4195e28a50c8227bdae0344d2110c8",
            "signature_scheme": "ECDSA_P384_SHA384",
            "attestation_status": "HARDWARE_VERIFIED"
        }),
    );

    produce(
        "sovereign-stream",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "telemetry_type": "GDPR_ARTICLE_17_RIGHT_TO_ERASURE",
            "erasure_receipt_id": "GDPR-ART17-ERASURE-20261101-89",
            "blinded_user_commitment": "0x8f2a93110de84cb019efaa349812cc912384910aaebfd012",
            "data_subject_category": "EU_CITIZEN_RECORDS",
            "proof_type": "ZERO_KNOWLEDGE_BLINDED_ERASURE_PROOF",
            "verification_status": "PERMANENTLY_REVOKED_AND_LOGGED"
        }),
    );

    // Partition 1
    produce(
        "sovereign-stream",
        1,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "telemetry_type": "ENTSO_E_CONTINENTAL_POWER_GRID_SYNC",
            "telemetry_station": "SUBSTATION-FRA-NORD-380KV",
            "grid_frequency_hz": 50.0028,
            "phase_angle_deviation_rad": -0.0014,
            "voltage_level_kv": 381.4,
            "active_power_flow_mw": 1450.2,
            "critical_infra_class": "NIS2_DIRECTIVE_ESSENTIAL_SERVICE"
        }),
    );

    produce(
        "sovereign-stream",
        1,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "telemetry_type": "BSI_IT_GRUNDSCHUTZ_COMPLIANCE_AUDIT",
            "audit_ref": "BSI-GRUNDSCHUTZ-SYS.1.3-2026",
            "tls_13_cipher_suites": ["TLS_AES_256_GCM_SHA384", "TLS_CHACHA20_POLY1305_SHA256"],
            "legacy_ciphers_accepted": 0,
            "forward_secrecy_mode": "DHE_X25519_MANDATORY",
            "compliance_rating": "100_PERCENT_COMPLIANT"
        }),
    );

    // Partition 2
    produce(
        "sovereign-stream",
        2,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "telemetry_type": "EU_DATA_ACT_DATA_RESIDENCY_HEARTBEAT",
            "jurisdiction": "EU_EEA_GERMANY",
            "sovereign_boundary_fencing": "HARDWARE_ENFORCED",
            "unauthorized_cross_border_egress_count": 0,
            "status": "PROVABLY_SOVEREIGN"
        }),
    );

    // =========================================================================
    // 3. Topic: healthcare-clinical-trials (Tenant: tenant-sovereign)
    // =========================================================================
    // Partition 0
    produce(
        "healthcare-clinical-trials",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "study_registry": "CLINICALTRIALS_GOV_NCT04859127",
            "protocol_id": "SOV-ONCO-P3-ADVAN-HER2",
            "pseudonymized_subject_hash": "0x84fae1011894bf29881092aafe01",
            "genomic_biomarker": "ERBB2",
            "variant_hgvs": "c.2313_2324dup (p.Ala775_Gly776insTyrValMetAla)",
            "clinical_significance": "PATHOGENIC_TIER_1",
            "sponsor": "European Clinical Oncology Network",
            "data_protection_class": "GDPR_ARTICLE_9_SPECIAL_CATEGORY_HEALTH_DATA"
        }),
    );

    produce(
        "healthcare-clinical-trials",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "study_registry": "EMA_CLINICAL_TRIAL_INFORMATION_SYSTEM",
            "ctis_application_id": "2026-501824-22-00",
            "investigational_medicinal_product": "IMP-SOV-814",
            "safety_event_type": "SUSPECTED_UNEXPECTED_SERIOUS_ADVERSE_REACTION",
            "reported_count": 0,
            "data_monitoring_committee_recommendation": "CONTINUE_TRIAL_UNMODIFIED"
        }),
    );

    // Partition 1
    produce(
        "healthcare-clinical-trials",
        1,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "telemetry_type": "CONTINUOUS_GLUCOSE_MONITORING_STREAM",
            "patient_identifier_token": "CGM-SOV-PATIENT-0914",
            "glucose_value_mg_dl": 114.2,
            "sensor_trend": "STABLE_FLAT",
            "calibration_factor": 0.994,
            "encryption_algorithm": "AES_256_GCM_SOVEREIGN_KEY"
        }),
    );

    // Partition 2
    produce(
        "healthcare-clinical-trials",
        2,
        "sovereign-class",
        Some(ResidencyPolicy::StrictLocal),
        json!({
            "audit_type": "POST_QUANTUM_KYBER1024_TRANSMISSION_SECURITY",
            "cipher_algorithm": "CRYSTALS-KYBER-1024-ROUND-3",
            "encapsulated_secret_size_bytes": 1568,
            "public_key_fingerprint": "PQ-KYBER-FRA-NODE-01",
            "round_trip_verification": "PASS_VERIFIED",
            "standard": "HIPAA_164_312_E_1_COMPLIANT"
        }),
    );

    // =========================================================================
    // 4. Topic: cross-border-kyc-registry (Tenant: tenant-sovereign)
    // =========================================================================
    // Partition 0
    produce(
        "cross-border-kyc-registry",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::RestrictedReplication(vec![RegionId::EuWest, RegionId::UsEast])),
        json!({
            "compliance_protocol": "FATF_RECOMMENDATION_16_TRAVEL_RULE",
            "transfer_id": "FATF-TR-20261101-99824",
            "originator_vasp_lei": "5493006MHB84DD0ZWV18",
            "beneficiary_vasp_lei": "984500982341ABCDEF12",
            "originator_kyc_status": "VERIFIED_FULL_LEVEL_3",
            "sanctions_screening_engine": "SOV_AML_SCREEN_V4",
            "screening_outcome": "ZERO_SDN_MATCH_PERMITTED"
        }),
    );

    produce(
        "cross-border-kyc-registry",
        0,
        "sovereign-class",
        Some(ResidencyPolicy::RestrictedReplication(vec![RegionId::EuWest, RegionId::UsEast])),
        json!({
            "compliance_protocol": "PEP_SANCTIONS_BATCH_SWEEP",
            "batch_id": "PEP-SWEEP-20261101-EU",
            "total_entities_checked": 14520,
            "positive_matches_flagged": 0,
            "watchlist_version": "EU_SANCTIONS_MAP_2026_11_01",
            "auditor_signoff": "COMPLIANCE_OFFICER_441"
        }),
    );

    // Partition 1
    produce(
        "cross-border-kyc-registry",
        1,
        "sovereign-class",
        Some(ResidencyPolicy::RestrictedReplication(vec![RegionId::EuWest, RegionId::UsEast])),
        json!({
            "trust_framework": "EIDAS_2_0_QUALIFIED_TRUST_SERVICE",
            "qtsp_name": "D-Trust GmbH (Bundesdruckerei Group)",
            "certificate_serial": "7721-0091-8824-5510",
            "signature_format": "PAdES_BASELINE_LTA",
            "qualified_timestamp_authority": "TSA.BUNDESDRUCKEREI.DE",
            "legal_validity": "CRYPTOGRAPHICALLY_VALID_EU_WIDE"
        }),
    );

    // Partition 2
    produce(
        "cross-border-kyc-registry",
        2,
        "sovereign-class",
        Some(ResidencyPolicy::RestrictedReplication(vec![RegionId::EuWest, RegionId::UsEast])),
        json!({
            "registry_type": "BENEFICIAL_OWNERSHIP_TRANSPARENCY",
            "corporate_registry_number": "HRB-981245-BERLIN",
            "company_name": "Sovereign Quantum Compute Labs GmbH",
            "ubo_shares_pct": 100.0,
            "ubo_citizenship": "DE",
            "registry_interconnect_status": "BORIS_EU_PORTAL_MATCH_FOUND"
        }),
    );

    // =========================================================================
    // 5. Topic: order-matching-engine (Tenant: tenant-default)
    // =========================================================================
    // Partition 0
    produce(
        "order-matching-engine",
        0,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "execution_type": "L3_LIMIT_ORDER_MATCH",
            "trade_id": "TRD-EURUSD-20261101-8914",
            "instrument": "EUR/USD",
            "side": "BUY",
            "price": 1.08425,
            "volume_units": 5000000,
            "buyer_order_id": "ORD-XETRA-99120",
            "seller_order_id": "ORD-LMAX-44129",
            "clearing_house": "EUREX_CLEARING_AG",
            "match_latency_ns": 420
        }),
    );

    produce(
        "order-matching-engine",
        0,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "market_data_type": "L3_ORDER_BOOK_DEPTH_UPDATE",
            "contract": "FDAX-DEC-2026",
            "top_bids": [
                {"price": 18450.5, "volume": 120},
                {"price": 18450.0, "volume": 240},
                {"price": 18449.5, "volume": 310}
            ],
            "top_asks": [
                {"price": 18451.0, "volume": 95},
                {"price": 18451.5, "volume": 180},
                {"price": 18452.0, "volume": 275}
            ],
            "spread_ticks": 1,
            "total_depth_contracts": 1220
        }),
    );

    // Partition 1
    produce(
        "order-matching-engine",
        1,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "message_type": "FIX_5_0_SP2_EXECUTION_REPORT_35_8",
            "cl_ord_id": "ORD-2026-X89211",
            "exec_type": "FILL",
            "ord_status": "FILLED",
            "symbol": "SAP.DE",
            "last_qty": 2500,
            "last_px": 184.20,
            "cum_qty": 2500,
            "leaves_qty": 0
        }),
    );

    produce(
        "order-matching-engine",
        1,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "execution_type": "DARK_POOL_MIDPOINT_CROSS",
            "cross_id": "MID-CROSS-20261101-0021",
            "venue_mic": "XFRA_DARK",
            "symbol": "ASML.AS",
            "shares": 14200,
            "mid_price": 874.50,
            "gross_notional_eur": 12417900.00,
            "market_impact_bps": 0.00
        }),
    );

    // Partition 2
    produce(
        "order-matching-engine",
        2,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "action": "HIGH_FREQUENCY_MARKET_MAKER_QUOTE_REPLACE",
            "packet_seq": 8941200,
            "market_maker_id": "MM-FLOW-TRADERS-FRA",
            "cancel_order_id": "ORD-99140",
            "new_order_id": "ORD-99141",
            "new_price": 18450.5,
            "turnaround_microseconds": 12
        }),
    );

    // =========================================================================
    // 6. Topic: general-topic (Tenant: tenant-default)
    // =========================================================================
    // Partition 0
    produce(
        "general-topic",
        0,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "event_type": "KUBERNETES_BARE_METAL_NODE_PROVISIONED",
            "cluster": "k8s-sovereign-fra-prod",
            "node_name": "worker-baremetal-042.fra.dc",
            "cpu_cores": 128,
            "memory_gib": 1024,
            "cgroup_v2_active": true,
            "status": "READY_FOR_WORKLOAD"
        }),
    );

    produce(
        "general-topic",
        0,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "event_type": "DISTRIBUTED_OPENTELEMETRY_TRACE_SPAN",
            "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
            "span_id": "00f067aa0ba902b7",
            "service_name": "kamish-broker-gateway",
            "http_method": "POST",
            "http_route": "/api/produce",
            "http_status": 200,
            "duration_ms": 0.38
        }),
    );

    // Partition 1
    produce(
        "general-topic",
        1,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "event_type": "SOVEREIGN_S3_PARQUET_ARCHIVE_UPLOADED",
            "bucket": "kamish-cold-archive-fra",
            "object_key": "year=2026/month=11/day=01/ledger_partition_00.parquet",
            "content_length_bytes": 4892140,
            "encryption": "aws:kms:arn:sov:kms:fra:key-42",
            "status": "DURABLY_STORED"
        }),
    );

    // Partition 2
    produce(
        "general-topic",
        2,
        "general",
        Some(ResidencyPolicy::GlobalCleartext),
        json!({
            "event_type": "SYSTEMD_WATCHDOG_OPERATIONAL_HEARTBEAT",
            "unit": "kamish-broker.service",
            "watchdog_interval_usec": 10000000,
            "rss_memory_mb": 42.5,
            "status": "WATCHDOG=1 PING SUCCESS"
        }),
    );

    // =========================================================================
    // 7. Commit Anchor Checkpoints to Metadata Authority
    // =========================================================================
    let last_hash = broker.enforcer.audit_log().last_hash();
    let count = broker.enforcer.audit_log().entry_count();
    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);

    let all_topics = [
        "financial-transactions",
        "sovereign-stream",
        "healthcare-clinical-trials",
        "cross-border-kyc-registry",
        "order-matching-engine",
        "general-topic",
    ];

    for topic in all_topics {
        for p in 0..3 {
            let stream = StreamId::new(topic, p);
            let ckpt = AuditCheckpoint {
                stream,
                sequence: 1,
                tail_hash: last_hash.clone(),
                record_count: count,
                signature_hex: "00".repeat(64),
                witness_timestamp_ns: now_ns,
            };
            let _ = broker.meta.commit_audit_checkpoint(ckpt);
        }
    }

    println!(
        "[kamish dev] Successfully seeded {} verified audit entries across 6 topics (18 partitions).",
        count
    );
}
