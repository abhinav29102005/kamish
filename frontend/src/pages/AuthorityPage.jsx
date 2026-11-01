import React, { useState, useEffect } from "react";
import gatewayClient from "../utils/gatewayClient";
import { useKamish } from "../context/useKamish";

export default function AuthorityPage() {
  const { currentUser, isLiveMode } = useKamish();
  const [authority, setAuthority] = useState(null);
  const [policy, setPolicy] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(null);

  // Policy publish state
  const [policyVersion, setPolicyVersion] = useState(2);

  const loadAuthorityData = async () => {
    setLoading(true);
    setError(null);
    try {
      if (gatewayClient.isAuthenticated()) {
        const [authRes, polRes] = await Promise.all([
          gatewayClient.getAuthority(),
          gatewayClient.getPolicy(),
        ]);
        setAuthority(authRes);
        setPolicy(polRes);
      } else {
        // Fallback demo state
        setAuthority({
          authority_type: "LOCAL (in-memory; quorum requires etcd)",
          consensus_backing: "LOCAL_IN_MEMORY",
          quorum_backed: false,
          active_region: "EU_WEST",
          monotonic_clock_ms: 1729482100000,
          ownerships: [
            { topic: "financial-transactions", partition: 0, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "financial-transactions", partition: 1, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "financial-transactions", partition: 2, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "sovereign-stream", partition: 0, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "sovereign-stream", partition: 1, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "sovereign-stream", partition: 2, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "healthcare-clinical-trials", partition: 0, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "healthcare-clinical-trials", partition: 1, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "healthcare-clinical-trials", partition: 2, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "cross-border-kyc-registry", partition: 0, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "cross-border-kyc-registry", partition: 1, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "cross-border-kyc-registry", partition: 2, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "order-matching-engine", partition: 0, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "order-matching-engine", partition: 1, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "order-matching-engine", partition: 2, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "general-topic", partition: 0, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "general-topic", partition: 1, owner_region: "EU_WEST", epoch: 1, active_lease: true },
            { topic: "general-topic", partition: 2, owner_region: "EU_WEST", epoch: 1, active_lease: true },
          ],
        });
        setPolicy({
          policy_version_floor: 1,
          enforcement: "StrictLocal / BorderRestricted Cryptographic Egress",
          registered_topics: [
            "financial-transactions",
            "sovereign-stream",
            "healthcare-clinical-trials",
            "cross-border-kyc-registry",
            "order-matching-engine",
            "general-topic",
          ],
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load metadata authority");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadAuthorityData();
  }, [currentUser]);

  const handlePublishPolicy = async (e) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const res = await gatewayClient.publishPolicy(Number(policyVersion));
      setSuccess(`Policy version floor advanced to ${res.new_policy_version_floor}! Broker nodes will reject stale policies.`);
      await loadAuthorityData();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to publish policy update");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ maxWidth: "1240px", margin: "0 auto", padding: "2rem 3.5rem 120px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "2rem", flexWrap: "wrap", gap: "1rem" }}>
        <div>
          <h1 style={{ fontFamily: "var(--font-mono)", fontSize: "2.5rem", fontWeight: 900, textTransform: "uppercase", color: "#ffffff", margin: 0 }}>
            METADATA AUTHORITY & SOVEREIGN POLICY
          </h1>
          <p style={{ fontFamily: "var(--font-mono)", color: "#a855f7", margin: "0.5rem 0 0" }}>
            Metadata Authority (LOCAL in-memory; quorum requires etcd) · Monotonic Lease Epochs · Signed Policy Floor
          </p>
        </div>

        <button
          onClick={loadAuthorityData}
          className="brutal-btn-primary"
          style={{ padding: "0.5rem 1.25rem", fontSize: "0.85rem" }}
        >
          REFRESH AUTHORITY
        </button>
      </div>

      {error && (
        <div role="alert" style={{ background: "rgba(244, 63, 94, 0.2)", border: "1px solid #f43f5e", color: "#fda4af", padding: "1rem", margin: "1rem 0", borderRadius: "8px", fontFamily: "var(--font-mono)", fontSize: "0.9rem" }}>
          {error}
        </div>
      )}

      {success && (
        <div style={{ background: "rgba(16, 185, 129, 0.2)", border: "1px solid #10b981", color: "#6ee7b7", padding: "1rem", margin: "1rem 0", borderRadius: "8px", fontFamily: "var(--font-mono)", fontSize: "0.9rem" }}>
          ✓ {success}
        </div>
      )}

      {/* TOP STATS CARDS */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))", gap: "1.5rem", marginBottom: "2.5rem" }}>
        <div className="brutal-card-offset" style={{ padding: "1.5rem" }}>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", display: "block" }}>
            AUTHORITY BACKEND
          </span>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "1.2rem", fontWeight: 700, color: "#38bdf8", display: "block", marginTop: "0.25rem" }}>
            {authority?.authority_type || "LOCAL (in-memory; quorum requires etcd)"}
          </span>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "0.8rem", color: "#10b981", marginTop: "0.5rem", display: "block" }}>
            ● Active Region: {authority?.active_region || "EU_WEST"}
          </span>
        </div>

        <div className="brutal-card-offset" style={{ padding: "1.5rem" }}>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", display: "block" }}>
            DURABLE POLICY VERSION FLOOR
          </span>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "1.8rem", fontWeight: 900, color: "#c084fc", display: "block", marginTop: "0.25rem" }}>
            v{policy?.policy_version_floor || 1}
          </span>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "0.8rem", color: "#d8b4fe", marginTop: "0.25rem", display: "block" }}>
            {policy?.enforcement || "StrictLocal Enforced"}
          </span>
        </div>

        <div className="brutal-card-offset" style={{ padding: "1.5rem" }}>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", display: "block" }}>
            MONOTONIC TIME
          </span>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "1.2rem", fontWeight: 700, color: "#34d399", display: "block", marginTop: "0.25rem" }}>
            {authority?.monotonic_clock_ms || Date.now()} ms
          </span>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: "0.8rem", color: "#94a3b8", marginTop: "0.5rem", display: "block" }}>
            Synchronized Monotonic Lease Check
          </span>
        </div>
      </div>

      {/* POLICY PUBLISHER (REQUIRES ADMIN) */}
      <div className="brutal-card-offset" style={{ padding: "1.75rem 2rem", marginBottom: "2.5rem" }}>
        <h3 style={{ fontFamily: "var(--font-mono)", color: "#c084fc", margin: "0 0 1rem", fontSize: "1.2rem" }}>
          PUBLISH SIGNED POLICY VERSION FLOOR (W11/W12)
        </h3>
        <p style={{ fontFamily: "var(--font-mono)", fontSize: "0.85rem", color: "#94a3b8", marginBottom: "1rem" }}>
          Advancing the policy floor permanently invalidates policies below this version across all broker nodes. Requires Admin role.
        </p>

        <form onSubmit={handlePublishPolicy} style={{ display: "flex", gap: "1rem", alignItems: "flex-end", flexWrap: "wrap" }}>
          <div style={{ width: "200px" }}>
            <label htmlFor="policy-ver" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#d8b4fe", marginBottom: "0.25rem" }}>
              NEW POLICY FLOOR
            </label>
            <input
              id="policy-ver"
              type="number"
              min={(policy?.policy_version_floor || 1) + 1}
              value={policyVersion}
              onChange={(e) => setPolicyVersion(e.target.value)}
              required
              style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
            />
          </div>

          <button
            type="submit"
            disabled={loading}
            className="brutal-btn-primary"
            style={{ padding: "0.6rem 1.5rem" }}
          >
            {loading ? "PUBLISHING..." : "PUBLISH POLICY UPDATE"}
          </button>
        </form>
      </div>

      {/* PARTITION OWNERSHIP TABLE */}
      <h3 style={{ fontFamily: "var(--font-mono)", color: "#ffffff", fontSize: "1.2rem", marginBottom: "1rem" }}>
        PARTITION LEASE OWNERSHIPS ({authority?.ownerships?.length || 0})
      </h3>
      <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        {(authority?.ownerships || []).map((o, idx) => (
          <div
            key={`${o.topic}-${o.partition}-${idx}`}
            style={{
              background: "#120b22",
              border: "1.5px solid rgba(168, 85, 247, 0.3)",
              borderRadius: "6px",
              padding: "1rem 1.5rem",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              flexWrap: "wrap",
              gap: "1rem",
              fontFamily: "var(--font-mono)",
            }}
          >
            <div>
              <span style={{ fontSize: "1rem", fontWeight: 700, color: "#ffffff" }}>
                {o.topic} : partition #{o.partition}
              </span>
              <span style={{ display: "block", fontSize: "0.8rem", color: "#94a3b8", marginTop: "0.2rem" }}>
                Owner Region: <strong style={{ color: "#38bdf8" }}>{o.owner_region}</strong>
              </span>
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: "1.5rem" }}>
              <span style={{ color: "#d8b4fe" }}>
                Epoch: <strong>{o.epoch}</strong>
              </span>

              <span
                style={{
                  fontSize: "0.75rem",
                  padding: "4px 10px",
                  borderRadius: "4px",
                  fontWeight: 700,
                  background: o.active_lease ? "rgba(16, 185, 129, 0.2)" : "rgba(244, 63, 94, 0.2)",
                  border: o.active_lease ? "1px solid #10b981" : "1px solid #f43f5e",
                  color: o.active_lease ? "#6ee7b7" : "#fda4af",
                }}
              >
                {o.active_lease ? "LEASE ACTIVE" : "LEASE EXPIRED"}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
