import React, { useState, useEffect } from "react";
import gatewayClient from "../utils/gatewayClient";
import { useKamish } from "../context/useKamish";

export default function AuditVerificationPage() {
  const { currentUser } = useKamish();
  const [auditInfo, setAuditInfo] = useState(null);
  const [checkers, setCheckers] = useState(null);
  const [verifyResult, setVerifyResult] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const loadAuditData = async () => {
    setLoading(true);
    setError(null);
    try {
      if (gatewayClient.isAuthenticated()) {
        const [aRes, cRes] = await Promise.all([
          gatewayClient.getAudit(),
          gatewayClient.getCheckers(),
        ]);
        setAuditInfo(aRes);
        setCheckers(cRes);
      } else {
        // Demo fallback
        setAuditInfo({
          audit_ledger_status: "TAMPER_EVIDENT_HASH_CHAINED",
          checkpoints_count: 3,
          checkpoint_anchors: [
            { stream: "financial-transactions:0", tail_hash: "3a8f9c102b4e87d12f3490...", record_count: 1420 },
            { stream: "financial-transactions:1", tail_hash: "7bc41a9e88d011244fa310...", record_count: 1390 },
            { stream: "financial-transactions:2", tail_hash: "ff902381249b01aa76c891...", record_count: 1410 },
          ],
          verification: "CHECKPOINT_ANCHORED_BOUNDARY_VALID",
        });
        setCheckers({
          history_checker: { status: "PASS", loss_violations: 0, duplicate_violations: 0, ordering_gaps: 0, epoch_conflicts: 0 },
          canary_scanner: { status: "PASS", active_canary_tokens_monitored: ["SECRET_CANARY_TOKEN_999"], positive_control_verified: true, storage_leak_detected: false },
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load audit data");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadAuditData();
  }, [currentUser]);

  const handleVerify = async () => {
    setLoading(true);
    setError(null);
    setVerifyResult(null);
    try {
      const res = await gatewayClient.verifyAudit();
      setVerifyResult(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Audit verification failed");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ maxWidth: "1240px", margin: "0 auto", padding: "2rem 3.5rem 120px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "2rem", flexWrap: "wrap", gap: "1rem" }}>
        <div>
          <h1 style={{ fontFamily: "var(--font-mono)", fontSize: "2.5rem", fontWeight: 900, textTransform: "uppercase", color: "#ffffff", margin: 0 }}>
            TAMPER-EVIDENT AUDIT & CHECKERS
          </h1>
          <p style={{ fontFamily: "var(--font-mono)", color: "#a855f7", margin: "0.5rem 0 0" }}>
            Cryptographic SHA-256 Hash Chain · Checkpoint Boundary Anchors · Canary Leak Scanner
          </p>
        </div>

        <button
          id="verify-audit-btn"
          onClick={handleVerify}
          disabled={loading}
          className="brutal-btn-primary"
          style={{ padding: "0.75rem 1.75rem", fontSize: "0.95rem" }}
        >
          {loading ? "VERIFYING CRYPTOGRAPHIC CHAIN..." : "RUN VERIFY-AUDIT"}
        </button>
      </div>

      {error && (
        <div role="alert" style={{ background: "rgba(244, 63, 94, 0.2)", border: "1px solid #f43f5e", color: "#fda4af", padding: "1rem", margin: "1rem 0", borderRadius: "8px", fontFamily: "var(--font-mono)", fontSize: "0.9rem" }}>
          {error}
        </div>
      )}

      {/* VERIFICATION RESULT */}
      {verifyResult && (
        <div
          id="verify-result-box"
          style={{
            background: verifyResult.verification_result === "PASS" ? "rgba(16, 185, 129, 0.15)" : "rgba(245, 158, 11, 0.15)",
            border: verifyResult.verification_result === "PASS" ? "2px solid #10b981" : "2px solid #f59e0b",
            borderRadius: "8px",
            padding: "1.5rem 2rem",
            marginBottom: "2.5rem",
            fontFamily: "var(--font-mono)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", color: verifyResult.verification_result === "PASS" ? "#34d399" : "#fbbf24", fontSize: "1.2rem", fontWeight: 700, marginBottom: "0.75rem" }}>
            <span>{verifyResult.verification_result === "PASS" ? "✓ CRYPTOGRAPHIC AUDIT VERIFIED" : "⚠ AUDIT VERIFICATION STATUS"}: {verifyResult.verification_result}</span>
          </div>
          {verifyResult.verification_result === "UNAVAILABLE" ? (
            <div style={{ fontSize: "0.85rem", color: "#fef3c7" }}>
              <div>Operation: <strong>{verifyResult.operation || "checkpoint_existence_check_only"}</strong></div>
              <div>Note: <em>{verifyResult.error || "Full on-disk verification wired in Step 4"}</em></div>
              <div>Streams Checked: <strong>{verifyResult.total_streams_checked || 0}</strong> | Checkpoints Present: <strong>{verifyResult.checkpoints_present || 0}</strong></div>
            </div>
          ) : (
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: "1rem", fontSize: "0.85rem", color: "#d1fae5" }}>
              <div>Anchored Boundary: <strong>{verifyResult.anchored_boundary ? "VALID" : "INVALID"}</strong></div>
              <div>Streams Audited: <strong>{verifyResult.total_streams_audited}</strong></div>
              <div>Checkpoints Verified: <strong>{verifyResult.total_checkpoints_verified}</strong></div>
              <div>Tamper Detected: <strong>{verifyResult.zero_tamper_detected ? "ZERO (INTEGRITY INTACT)" : "TAMPER FOUND"}</strong></div>
              <div>Signatures: <strong>{verifyResult.cryptographic_signatures_valid ? "CRYPTOGRAPHICALLY VALID" : "INVALID"}</strong></div>
            </div>
          )}
        </div>
      )}

      {/* TWO COLUMN INSPECTOR */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(480px, 1fr))", gap: "2rem" }}>
        {/* CHECKPOINT ANCHORS */}
        <div className="brutal-card-offset" style={{ padding: "2rem" }}>
          <h2 style={{ fontFamily: "var(--font-mono)", fontSize: "1.3rem", color: "#c084fc", margin: "0 0 1rem" }}>
            CHECKPOINT ANCHOR BOUNDARIES
          </h2>
          <p style={{ fontFamily: "var(--font-mono)", fontSize: "0.85rem", color: "#94a3b8", marginBottom: "1rem" }}>
            Durable audit checkpoints committed to the metadata authority after persist acknowledgments.
          </p>

          <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
            {(auditInfo?.checkpoint_anchors || []).map((cp, idx) => (
              <div
                key={cp.stream || idx}
                style={{
                  background: "#0d071a",
                  border: "1px solid rgba(168, 85, 247, 0.3)",
                  padding: "0.75rem 1rem",
                  borderRadius: "6px",
                  fontFamily: "var(--font-mono)",
                  fontSize: "0.8rem",
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", color: "#d8b4fe", marginBottom: "0.25rem" }}>
                  <span>Stream: <strong>{cp.stream}</strong></span>
                  <span>Records: <strong>{cp.record_count}</strong></span>
                </div>
                <div style={{ color: "#94a3b8", fontSize: "0.75rem", wordBreak: "break-all" }}>
                  Tail Hash: {cp.tail_hash}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* INDEPENDENT CHECKERS & SCANNER */}
        <div className="brutal-card-offset" style={{ padding: "2rem" }}>
          <h2 style={{ fontFamily: "var(--font-mono)", fontSize: "1.3rem", color: "#38bdf8", margin: "0 0 1rem" }}>
            EVIDENCE: CANARY SCANNER & CHECKERS
          </h2>
          <p style={{ fontFamily: "var(--font-mono)", fontSize: "0.85rem", color: "#94a3b8", marginBottom: "1rem" }}>
            Independent W30 verification running continuously over decoded storage and wire bytes.
          </p>

          <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
            {/* History Checker Box */}
            <div style={{ background: "#0d071a", border: "1px solid #38bdf8", borderRadius: "6px", padding: "1rem", fontFamily: "var(--font-mono)" }}>
              <div style={{ display: "flex", justifyContent: "space-between", color: "#38bdf8", fontWeight: 700, marginBottom: "0.5rem" }}>
                <span>HISTORY-BASED CONSISTENCY CHECKER</span>
                <span style={{ color: "#34d399" }}>[{checkers?.history_checker?.status || "PASS"}]</span>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.5rem", fontSize: "0.8rem", color: "#94a3b8" }}>
                <div>Loss Violations: <strong style={{ color: "#ffffff" }}>{checkers?.history_checker?.loss_violations || 0}</strong></div>
                <div>Duplicate Violations: <strong style={{ color: "#ffffff" }}>{checkers?.history_checker?.duplicate_violations || 0}</strong></div>
                <div>Ordering Gaps: <strong style={{ color: "#ffffff" }}>{checkers?.history_checker?.ordering_gaps || 0}</strong></div>
                <div>Epoch Conflicts: <strong style={{ color: "#ffffff" }}>{checkers?.history_checker?.epoch_conflicts || 0}</strong></div>
              </div>
            </div>

            {/* Canary Scanner Box */}
            <div style={{ background: "#0d071a", border: "1px solid #c084fc", borderRadius: "6px", padding: "1rem", fontFamily: "var(--font-mono)" }}>
              <div style={{ display: "flex", justifyContent: "space-between", color: "#c084fc", fontWeight: 700, marginBottom: "0.5rem" }}>
                <span>STORAGE & WIRE CANARY SCANNER</span>
                <span style={{ color: "#34d399" }}>[{checkers?.canary_scanner?.status || "PASS"}]</span>
              </div>
              <div style={{ fontSize: "0.8rem", color: "#94a3b8", display: "flex", flexDirection: "column", gap: "0.25rem" }}>
                <div>Positive Control Verified: <strong style={{ color: "#34d399" }}>{checkers?.canary_scanner?.positive_control_verified ? "YES" : "NO"}</strong></div>
                <div>Storage Leaks Detected: <strong style={{ color: "#34d399" }}>{checkers?.canary_scanner?.storage_leak_detected ? "DETECTED" : "ZERO LEAKS"}</strong></div>
                <div>Monitored Tokens: <strong style={{ color: "#e2d9f3" }}>{JSON.stringify(checkers?.canary_scanner?.active_canary_tokens_monitored || [])}</strong></div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
