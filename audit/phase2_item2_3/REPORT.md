# Phase 2 — Item 2.3 Audit Report: W10 Tenant Catalog & W13 Authenticated Consumer Fetch

**Execution Timestamp:** 2026-11-01T05:50:00+05:30  
**Status:** COMPLETE (Item 2.3)

---

## 1. Item + Files Modified

### Implemented Capabilities (W10 & W13):
1. **Authorized Tenant & Topic Catalog (`TenantCatalog` / `TopicMetadata`):**
   - Implemented `TenantCatalog` and `TopicMetadata` in `cmd/kamish-broker/src/catalog.rs`.
   - Each topic in the catalog declares:
     - `owner_tenant`: Tenant that owns and controls the topic.
     - `allowed_classes`: Strictly whitelisted data classification IDs that may be written or fetched.
     - `authorized_producers`: Whitelisted principals allowed to publish to this topic.
     - `authorized_consumers`: Whitelisted principals allowed to consume from this topic.
     - `allowed_consumer_regions`: Strictly whitelisted regions from which consumers may fetch data (sovereignty boundary).
2. **Producers Cannot Choose Another Tenant's Class (W10):**
   - In `BrokerServer::produce_with_identity`:
     - Cross-tenant injection is rejected: if `identity.tenant != topic.owner_tenant`, returns `403 PolicyDenied (Tenant mismatch)`.
     - Unauthorized class injection is rejected: if `!topic.allowed_classes.contains(&req.class_id)`, returns `403 PolicyDenied (Data class denied)`. Producers cannot choose another tenant's or unauthorized class.
     - Unauthorized principal is rejected if `authorized_producers` list is non-empty.
3. **Consumer Fetch Bound to Authenticated Consumer Identity, Region, and Role (W13):**
   - In `BrokerServer::fetch_with_identity`:
     - Cross-tenant fetch is rejected: if `identity.tenant != topic.owner_tenant`, returns `403 PolicyDenied (Tenant mismatch)`.
     - Region boundary violation is rejected: if `!topic.allowed_consumer_regions.contains(&identity.region)`, returns `403 PolicyDenied (Region boundary denied)`. Consumers in unpermitted regions cannot fetch regional data.
     - Unauthorized consumer principal is rejected if `authorized_consumers` list is non-empty.
     - Class boundary check ensures consumer requested class matches topic allowed classes and disk records.
4. **End-to-End Async Network Integration:**
   - Updated `BrokerServer::handle_stream` to pass authenticated `ClientIdentity` into worker threads.
   - Dispatches via `dispatch_with_identity` for thread-safe asynchronous processing.

### Files Modified / Added:
- `cmd/kamish-broker/src/catalog.rs`: Defined `TenantCatalog` and `TopicMetadata`.
- `cmd/kamish-broker/src/config.rs`: Added `TopicConfig` and `BrokerConfig::build_catalog`.
- `cmd/kamish-broker/src/lib.rs`: Integrated `catalog` into `BrokerServer`, implemented `produce_with_identity`, `fetch_with_identity`, and `dispatch_with_identity`.
- `cmd/kamish-broker/tests/tenant_catalog_and_fetch.rs`: Added integration tests verifying W10 and W13.

---

## 2. Tests Added + Exact Commands + Real Output

### Integration Tests (`cmd/kamish-broker/tests/tenant_catalog_and_fetch.rs`):
- **Scenario 1:** Authorized fintech producer in `EU_WEST` publishes valid `fintech-transfer` class payload -> `ProduceResponse` ACK received.
- **Scenario 2 (W10):** Producer attempts to choose another tenant's class (`medical-records` belonging to healthcorp) -> rejected with `Data class denied: class 'medical-records' is not authorized for topic 'payments'`.
- **Scenario 3 (W10):** Healthcorp producer attempts to produce to fintech's topic -> rejected with `Tenant mismatch: producer tenant 'healthcorp' cannot produce to topic 'payments' owned by 'fintech'`.
- **Scenario 4 (W13):** Authorized fintech consumer in `EU_WEST` fetches records -> successfully receives records.
- **Scenario 5 (W13):** Consumer in `US_EAST` attempts to fetch from `EU_WEST`-restricted `payments` topic -> rejected with `Region boundary denied: consumer in region 'US_EAST' cannot fetch topic 'payments' restricted to regions {"EU_WEST"}`.

### Exact Commands Run:
1. `cargo test --test tenant_catalog_and_fetch`: 1/1 passed.
2. `cargo test --workspace`: 100% passed across all crates.
3. `cargo test --manifest-path early_research/probes/Cargo.toml`: 17/17 passed.
4. `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, clean.
5. `cargo fmt --check`: Clean formatting.

Evidence captured in `audit/phase2_item2_3/`.
