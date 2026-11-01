import React, { useState, useEffect } from "react";
import gatewayClient from "../utils/gatewayClient";
import { useKamish } from "../context/useKamish";

export default function TopicsPage() {
  const { currentUser, isLiveMode } = useKamish();
  const [topics, setTopics] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(null);

  // New topic form state
  const [newTopic, setNewTopic] = useState("");
  const [partitionCount, setPartitionCount] = useState(3);
  const [classId, setClassId] = useState("sovereign-class");

  const loadTopics = async () => {
    setLoading(true);
    setError(null);
    try {
      if (gatewayClient.isAuthenticated()) {
        const res = await gatewayClient.getTopics();
        setTopics(res.topics || []);
      } else {
        // Fallback demo catalog
        setTopics([
          { topic_name: "financial-transactions", owner_tenant: "tenant-sovereign", partition_count: 3, allowed_classes: ["sovereign-class"] },
          { topic_name: "sovereign-stream", owner_tenant: "tenant-sovereign", partition_count: 3, allowed_classes: ["sovereign-class"] },
          { topic_name: "general-topic", owner_tenant: "tenant-default", partition_count: 3, allowed_classes: ["general"] },
        ]);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load topics");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadTopics();
  }, [currentUser]);

  const handleCreateTopic = async (e) => {
    e.preventDefault();
    if (!newTopic.trim()) return;
    setError(null);
    setSuccess(null);
    setLoading(true);
    try {
      const res = await gatewayClient.createTopic(newTopic.trim(), Number(partitionCount), classId);
      setSuccess(`Topic "${res.topic}" successfully created with ${res.partitions} partitions in class "${res.class_id}"`);
      setNewTopic("");
      await loadTopics();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Topic creation failed");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ maxWidth: "1240px", margin: "0 auto", padding: "2rem 3.5rem 120px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "2rem", flexWrap: "wrap", gap: "1rem" }}>
        <div>
          <h1 style={{ fontFamily: "var(--font-mono)", fontSize: "2.5rem", fontWeight: 900, textTransform: "uppercase", color: "#ffffff", margin: 0 }}>
            TOPIC & STREAM CATALOG
          </h1>
          <p style={{ fontFamily: "var(--font-mono)", color: "#a855f7", margin: "0.5rem 0 0" }}>
            Authorized Multi-Tenant Topic Catalog · W10 Class Boundary Enforced
          </p>
        </div>

        <button
          onClick={loadTopics}
          className="brutal-btn-primary"
          style={{ padding: "0.5rem 1.25rem", fontSize: "0.85rem" }}
        >
          REFRESH CATALOG
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

      {/* Topic Creation Card (Requires Admin or Operator) */}
      <div className="brutal-card-offset" style={{ padding: "1.75rem 2rem", marginBottom: "2.5rem" }}>
        <h3 style={{ fontFamily: "var(--font-mono)", color: "#c084fc", margin: "0 0 1rem", fontSize: "1.2rem" }}>
          + CREATE NEW SOVEREIGN TOPIC
        </h3>
        <form onSubmit={handleCreateTopic} style={{ display: "flex", flexWrap: "wrap", gap: "1rem", alignItems: "flex-end" }}>
          <div style={{ flex: "1 1 240px" }}>
            <label htmlFor="topic-name" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#d8b4fe", marginBottom: "0.25rem" }}>
              TOPIC NAME
            </label>
            <input
              id="topic-name"
              type="text"
              value={newTopic}
              onChange={(e) => setNewTopic(e.target.value)}
              placeholder="e.g. crossborder-payments"
              required
              style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
            />
          </div>

          <div style={{ width: "120px" }}>
            <label htmlFor="part-count" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#d8b4fe", marginBottom: "0.25rem" }}>
              PARTITIONS
            </label>
            <input
              id="part-count"
              type="number"
              min="1"
              max="16"
              value={partitionCount}
              onChange={(e) => setPartitionCount(e.target.value)}
              required
              style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
            />
          </div>

          <div style={{ width: "180px" }}>
            <label htmlFor="class-select" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#d8b4fe", marginBottom: "0.25rem" }}>
              ALLOWED CLASS
            </label>
            <select
              id="class-select"
              value={classId}
              onChange={(e) => setClassId(e.target.value)}
              style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
            >
              <option value="sovereign-class">sovereign-class</option>
              <option value="general">general</option>
              <option value="fintech-transfer">fintech-transfer</option>
            </select>
          </div>

          <button
            type="submit"
            disabled={loading}
            className="brutal-btn-primary"
            style={{ padding: "0.6rem 1.5rem", fontSize: "0.9rem" }}
          >
            {loading ? "REGISTERING..." : "REGISTER TOPIC"}
          </button>
        </form>
      </div>

      {/* Catalog Table */}
      <h3 style={{ fontFamily: "var(--font-mono)", color: "#ffffff", fontSize: "1.2rem", marginBottom: "1rem" }}>
        ACTIVE CATALOG TOPICS ({topics.length})
      </h3>
      <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
        {topics.map((t, idx) => (
          <div
            key={t.topic || t.topic_name || idx}
            style={{
              background: "#120b22",
              border: "1.5px solid rgba(168, 85, 247, 0.4)",
              padding: "1.25rem 1.75rem",
              borderRadius: "8px",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              flexWrap: "wrap",
              gap: "1rem",
            }}
          >
            <div>
              <span style={{ fontFamily: "var(--font-mono)", fontSize: "1.1rem", fontWeight: 700, color: "#ffffff", display: "block" }}>
                {t.topic || t.topic_name}
              </span>
              <span style={{ fontFamily: "var(--font-mono)", fontSize: "0.8rem", color: "#94a3b8" }}>
                Owner Tenant: <strong style={{ color: "#d8b4fe" }}>{t.owner_tenant}</strong> · Partitions: <strong>{t.partitions || t.partition_count}</strong>
              </span>
            </div>

            <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
              {(t.allowed_classes || []).map((c) => (
                <span
                  key={c}
                  style={{
                    fontFamily: "var(--font-mono)",
                    fontSize: "0.75rem",
                    background: "rgba(168, 85, 247, 0.2)",
                    border: "1px solid #a855f7",
                    color: "#e9d5ff",
                    padding: "3px 8px",
                    borderRadius: "4px",
                  }}
                >
                  class: {c}
                </span>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
