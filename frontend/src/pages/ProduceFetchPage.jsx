import React, { useState } from "react";
import gatewayClient from "../utils/gatewayClient";
import { useKamish } from "../context/useKamish";

export default function ProduceFetchPage() {
  const { currentUser } = useKamish();

  // Producer state
  const [prodTopic, setProdTopic] = useState("financial-transactions");
  const [prodPartition, setProdPartition] = useState(0);
  const [prodClass, setProdClass] = useState("sovereign-class");
  const [prodPayload, setProdPayload] = useState("TRANSACTION_ID=TX-9001 AMOUNT=50000 CURRENCY=EUR");
  const [prodLoading, setProdLoading] = useState(false);
  const [prodResult, setProdResult] = useState(null);
  const [prodError, setProdError] = useState(null);

  // Consumer state
  const [fetchTopic, setFetchTopic] = useState("financial-transactions");
  const [fetchPartition, setFetchPartition] = useState(0);
  const [fetchOffset, setFetchOffset] = useState(0);
  const [fetchMax, setFetchMax] = useState(10);
  const [fetchClass, setFetchClass] = useState("sovereign-class");
  const [fetchLoading, setFetchLoading] = useState(false);
  const [fetchResult, setFetchResult] = useState(null);
  const [fetchError, setFetchError] = useState(null);

  const handleProduce = async (e) => {
    e.preventDefault();
    setProdLoading(true);
    setProdResult(null);
    setProdError(null);
    try {
      const res = await gatewayClient.produce(
        prodTopic,
        Number(prodPartition),
        prodClass,
        prodPayload
      );
      setProdResult(res);
    } catch (err) {
      setProdError(err instanceof Error ? err.message : "Produce failed");
    } finally {
      setProdLoading(false);
    }
  };

  const handleFetch = async (e) => {
    e.preventDefault();
    setFetchLoading(true);
    setFetchResult(null);
    setFetchError(null);
    try {
      const res = await gatewayClient.fetch(
        fetchTopic,
        Number(fetchPartition),
        Number(fetchOffset),
        Number(fetchMax),
        fetchClass
      );
      setFetchResult(res);
    } catch (err) {
      setFetchError(err instanceof Error ? err.message : "Fetch failed");
    } finally {
      setFetchLoading(false);
    }
  };

  return (
    <div style={{ maxWidth: "1240px", margin: "0 auto", padding: "2rem 3.5rem 120px" }}>
      <div style={{ marginBottom: "2rem" }}>
        <h1 style={{ fontFamily: "var(--font-mono)", fontSize: "2.5rem", fontWeight: 900, textTransform: "uppercase", color: "#ffffff", margin: 0 }}>
          PRODUCE & FETCH CONSOLE
        </h1>
        <p style={{ fontFamily: "var(--font-mono)", color: "#a855f7", margin: "0.5rem 0 0" }}>
          Direct Interactive Ingress & Egress over Authenticated Gateway API
        </p>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(480px, 1fr))", gap: "2rem" }}>
        {/* PRODUCER CARD */}
        <div className="brutal-card-offset" style={{ padding: "2rem" }}>
          <h2 style={{ fontFamily: "var(--font-mono)", fontSize: "1.3rem", color: "#38bdf8", margin: "0 0 1rem" }}>
            1. REAL RECORD PRODUCER
          </h2>

          {prodError && (
            <div role="alert" style={{ background: "rgba(244, 63, 94, 0.2)", border: "1px solid #f43f5e", color: "#fda4af", padding: "0.75rem", marginBottom: "1rem", borderRadius: "6px", fontFamily: "var(--font-mono)", fontSize: "0.85rem" }}>
              {prodError}
            </div>
          )}

          {prodResult && (
            <div style={{ background: "rgba(16, 185, 129, 0.15)", border: "1px solid #10b981", color: "#a7f3d0", padding: "1rem", marginBottom: "1rem", borderRadius: "6px", fontFamily: "var(--font-mono)", fontSize: "0.85rem" }}>
              <div style={{ fontWeight: 700, color: "#34d399", marginBottom: "0.25rem" }}>ACKNOWLEDGED BY BROKER</div>
              <div>Base Offset: <strong>{prodResult.base_offset}</strong></div>
              <div>Count: <strong>{prodResult.count}</strong></div>
              <div>Committed Epoch: <strong>{prodResult.committed_epoch}</strong></div>
              <div>Sovereignty Policy: <strong>{prodResult.sovereignty_allowed ? "ALLOWED" : "DENIED"}</strong></div>
            </div>
          )}

          <form onSubmit={handleProduce} style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
            <div>
              <label htmlFor="prod-topic" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                TOPIC
              </label>
              <select
                id="prod-topic"
                value={prodTopic}
                onChange={(e) => setProdTopic(e.target.value)}
                style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #38bdf8", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
              >
                <option value="financial-transactions">financial-transactions</option>
                <option value="sovereign-stream">sovereign-stream</option>
                <option value="general-topic">general-topic</option>
              </select>
            </div>

            <div style={{ display: "flex", gap: "1rem" }}>
              <div style={{ flex: 1 }}>
                <label htmlFor="prod-part" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                  PARTITION
                </label>
                <input
                  id="prod-part"
                  type="number"
                  min="0"
                  max="15"
                  value={prodPartition}
                  onChange={(e) => setProdPartition(e.target.value)}
                  style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #38bdf8", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
                />
              </div>

              <div style={{ flex: 2 }}>
                <label htmlFor="prod-class" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                  DATA CLASS
                </label>
                <select
                  id="prod-class"
                  value={prodClass}
                  onChange={(e) => setProdClass(e.target.value)}
                  style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #38bdf8", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
                >
                  <option value="sovereign-class">sovereign-class (StrictLocal)</option>
                  <option value="general">general (MultiRegion)</option>
                  <option value="fintech-transfer">fintech-transfer</option>
                </select>
              </div>
            </div>

            <div>
              <label htmlFor="prod-payload" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                PAYLOAD (UTF-8 TEXT OR JSON)
              </label>
              <textarea
                id="prod-payload"
                rows={3}
                value={prodPayload}
                onChange={(e) => setProdPayload(e.target.value)}
                style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #38bdf8", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
              />
            </div>

            <button
              id="submit-produce-btn"
              type="submit"
              disabled={prodLoading}
              className="brutal-btn-primary"
              style={{ background: "#0284c7", borderColor: "#38bdf8" }}
            >
              {prodLoading ? "PUBLISHING TO BROKER..." : "PRODUCE MESSAGE"}
            </button>
          </form>
        </div>

        {/* CONSUMER CARD */}
        <div className="brutal-card-offset" style={{ padding: "2rem" }}>
          <h2 style={{ fontFamily: "var(--font-mono)", fontSize: "1.3rem", color: "#c084fc", margin: "0 0 1rem" }}>
            2. REAL RECORD FETCH CONSOLE
          </h2>

          {fetchError && (
            <div role="alert" style={{ background: "rgba(244, 63, 94, 0.2)", border: "1px solid #f43f5e", color: "#fda4af", padding: "0.75rem", marginBottom: "1rem", borderRadius: "6px", fontFamily: "var(--font-mono)", fontSize: "0.85rem" }}>
              {fetchError}
            </div>
          )}

          <form onSubmit={handleFetch} style={{ display: "flex", flexDirection: "column", gap: "1rem", marginBottom: "1.5rem" }}>
            <div>
              <label htmlFor="fetch-topic" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                TOPIC
              </label>
              <select
                id="fetch-topic"
                value={fetchTopic}
                onChange={(e) => setFetchTopic(e.target.value)}
                style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
              >
                <option value="financial-transactions">financial-transactions</option>
                <option value="sovereign-stream">sovereign-stream</option>
                <option value="general-topic">general-topic</option>
              </select>
            </div>

            <div style={{ display: "flex", gap: "1rem" }}>
              <div style={{ flex: 1 }}>
                <label htmlFor="fetch-part" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                  PARTITION
                </label>
                <input
                  id="fetch-part"
                  type="number"
                  min="0"
                  max="15"
                  value={fetchPartition}
                  onChange={(e) => setFetchPartition(e.target.value)}
                  style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
                />
              </div>

              <div style={{ flex: 1 }}>
                <label htmlFor="fetch-offset" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                  START OFFSET
                </label>
                <input
                  id="fetch-offset"
                  type="number"
                  min="0"
                  value={fetchOffset}
                  onChange={(e) => setFetchOffset(e.target.value)}
                  style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
                />
              </div>

              <div style={{ flex: 1 }}>
                <label htmlFor="fetch-max" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.75rem", color: "#94a3b8", marginBottom: "0.25rem" }}>
                  MAX RECORDS
                </label>
                <input
                  id="fetch-max"
                  type="number"
                  min="1"
                  max="50"
                  value={fetchMax}
                  onChange={(e) => setFetchMax(e.target.value)}
                  style={{ width: "100%", padding: "0.6rem 0.8rem", background: "#0d071a", border: "1px solid #a855f7", color: "#fff", fontFamily: "var(--font-mono)", borderRadius: "4px" }}
                />
              </div>
            </div>

            <button
              id="submit-fetch-btn"
              type="submit"
              disabled={fetchLoading}
              className="brutal-btn-primary"
            >
              {fetchLoading ? "FETCHING FROM BROKER..." : "FETCH RECORDS"}
            </button>
          </form>

          {/* FETCH RESULTS TABLE */}
          {fetchResult && (
            <div>
              <div style={{ fontFamily: "var(--font-mono)", fontSize: "0.85rem", color: "#c084fc", marginBottom: "0.75rem" }}>
                High Watermark: <strong>{fetchResult.high_watermark}</strong> · Records Returned: <strong>{fetchResult.records?.length || 0}</strong>
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem", maxHeight: "280px", overflowY: "auto" }}>
                {(fetchResult.records || []).map((rec) => (
                  <div
                    key={rec.offset}
                    style={{
                      background: "#0d071a",
                      border: "1px solid rgba(168, 85, 247, 0.3)",
                      borderRadius: "4px",
                      padding: "0.6rem 0.8rem",
                      fontFamily: "var(--font-mono)",
                      fontSize: "0.8rem",
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", color: "#d8b4fe", marginBottom: "0.25rem" }}>
                      <span>Offset: <strong>#{rec.offset}</strong></span>
                      <span>Epoch: <strong>{rec.epoch}</strong></span>
                      <span style={{ color: "#38bdf8" }}>[{rec.class_id}]</span>
                    </div>
                    <div style={{ color: "#ffffff", wordBreak: "break-all" }}>
                      {rec.payload}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
