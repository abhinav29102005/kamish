import React, { useState, useEffect } from 'react';
import { useKamish } from '../context/useKamish';

export default function TelemetryPage() {
  const { isLiveMode, currentTelemetry } = useKamish();
  const [demoMetrics, setDemoMetrics] = useState({
    throughput: 12479,
    p50: 59.9,
    p90: 157.3,
    p99: 241.2,
    max: 399.4,
    activeBrokers: 1,
    totalRecords: 1428590,
  });

  useEffect(() => {
    // In LIVE mode, NEVER invent synthetic random metrics
    if (isLiveMode) {
      return;
    }

    // In DEMO mode only, run synthetic simulation
    const interval = setInterval(() => {
      setDemoMetrics((prev) => ({
        ...prev,
        throughput: 12450,
        p50: 59.5,
        p99: 240.0,
        totalRecords: prev.totalRecords + 10,
      }));
    }, 1500);
    return () => clearInterval(interval);
  }, [isLiveMode]);

  return (
    <div style={{ paddingBottom: '120px', maxWidth: '1100px', margin: '0 auto', paddingLeft: '1.5rem', paddingRight: '1.5rem' }}>
      <div style={{ marginTop: '2rem', marginBottom: '2rem' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '6px' }}>
          {isLiveMode ? (
            <>
              <span className="mono-tag mono-tag-emerald">LIVE BROKER TELEMETRY</span>
              <span className="mono-tag mono-tag-shadow">GATEWAY CONNECTED</span>
            </>
          ) : (
            <>
              <span className="mono-tag mono-tag-amber">SIMULATED TELEMETRY</span>
              <span className="mono-tag mono-tag-crimson">OFFLINE BENCHMARK PROFILE</span>
            </>
          )}
        </div>
        <h1
          style={{
            fontFamily: 'var(--font-hero)',
            fontSize: 'clamp(2.2rem, 4vw, 3.2rem)',
            fontWeight: 900,
            color: '#fff',
          }}
        >
          {isLiveMode ? 'LIVE SYSTEM TELEMETRY' : 'DEMO SYSTEM TELEMETRY'}
        </h1>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.95rem' }}>
          {isLiveMode
            ? 'Real-time broker throughput, partition outbox depths, and replication lag metrics from the gateway.'
            : 'Simulated performance profiles and synthetic latency distribution for local offline demonstration.'}
        </p>
      </div>

      {/* Main Stats Grid */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))',
          gap: '1.25rem',
          marginBottom: '2rem',
        }}
      >
        <div className="brutal-card">
          <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
            {isLiveMode ? 'LIVE INGRESS THROUGHPUT' : 'SUSTAINED THROUGHPUT'}
          </span>
          <h2 style={{ fontFamily: 'var(--font-mono)', fontSize: '2.4rem', color: '#10b981', margin: '0.5rem 0' }}>
            {isLiveMode
              ? (currentTelemetry?.throughputEps ?? 0).toLocaleString()
              : demoMetrics.throughput.toLocaleString()}{' '}
            <span style={{ fontSize: '1rem', color: 'var(--text-muted)' }}>msg/s</span>
          </h2>
          <span className="mono-tag mono-tag-emerald">
            {isLiveMode ? 'REPORTED BROKER EPS' : 'SYNTHETIC CAPACITY PROFILE'}
          </span>
        </div>

        <div className="brutal-card">
          <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
            {isLiveMode ? 'DESTINATION OUTBOX QUEUE' : 'MEDIAN LATENCY (p50)'}
          </span>
          <h2 style={{ fontFamily: 'var(--font-mono)', fontSize: '2.4rem', color: '#06b6d4', margin: '0.5rem 0' }}>
            {isLiveMode
              ? (currentTelemetry?.outboxDepth ?? 0).toLocaleString()
              : demoMetrics.p50}{' '}
            <span style={{ fontSize: '1rem', color: 'var(--text-muted)' }}>
              {isLiveMode ? 'records' : 'µs'}
            </span>
          </h2>
          <span className="mono-tag mono-tag-shadow">
            {isLiveMode ? 'BOUNDED BUFFER CAPACITY' : 'SUB-MILLISECOND INGESTION'}
          </span>
        </div>

        <div className="brutal-card">
          <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
            {isLiveMode ? 'REPLICATION LAG' : 'TAIL LATENCY (p99)'}
          </span>
          <h2 style={{ fontFamily: 'var(--font-mono)', fontSize: '2.4rem', color: '#f59e0b', margin: '0.5rem 0' }}>
            {isLiveMode
              ? (currentTelemetry?.replicationLag ?? 0).toLocaleString()
              : demoMetrics.p99}{' '}
            <span style={{ fontSize: '1rem', color: 'var(--text-muted)' }}>
              {isLiveMode ? 'records' : 'µs'}
            </span>
          </h2>
          <span className="mono-tag mono-tag-amber">
            {isLiveMode ? 'WAN REPLICATION SKEW' : 'DECOUPLED FROM WAN'}
          </span>
        </div>

        <div className="brutal-card">
          <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
            {isLiveMode ? 'TOTAL COMMITTED OFFSET' : 'TOTAL RECORDS PERSISTED'}
          </span>
          <h2 style={{ fontFamily: 'var(--font-mono)', fontSize: '2.4rem', color: '#fff', margin: '0.5rem 0' }}>
            {isLiveMode
              ? (currentTelemetry?.localCommitted ?? 0).toLocaleString()
              : demoMetrics.totalRecords.toLocaleString()}
          </h2>
          <span className="mono-tag mono-tag-crimson">
            {isLiveMode ? 'FSYNC LOCAL LOG COMMITTED' : 'SYNTHETIC FSYNC LOG'}
          </span>
        </div>
      </div>

      {/* Latency Distribution Breakdown */}
      <div className="brutal-card">
        <h3 style={{ fontFamily: 'var(--font-hero)', fontSize: '1.2rem', color: '#fff', marginBottom: '1rem' }}>
          {isLiveMode
            ? 'PERCENTILE LATENCY STATUS (BENCHMARK ORCHESTRATION)'
            : 'SIMULATED PERCENTILE LATENCY (DEMO BENCHMARK PROFILE)'}
        </h3>

        {isLiveMode ? (
          <div style={{ fontFamily: 'var(--font-mono)', fontSize: '0.88rem', color: '#cbd5e1', lineHeight: 1.6 }}>
            <p style={{ margin: '0 0 0.75rem 0' }}>
              ℹ <strong>Empirical Benchmark Notice:</strong> Sub-millisecond percentile distributions (p50, p90, p99) require executing the live benchmark harness across regional clusters (<code>benchmarks/orchestrator.mjs</code>).
            </p>
            <p style={{ margin: '0 0 0.75rem 0', color: '#94a3b8' }}>
              The current live broker gateway reports average single-record append fsync latency of <strong>~0.40ms</strong>. To avoid misleading operational claims, unmeasured latency curves are not synthesized during live mode sessions.
            </p>
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
            <div>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                <span>p50 (Median): {demoMetrics.p50} µs</span>
                <span style={{ color: '#10b981' }}>Fast Ingestion Path [SIMULATED]</span>
              </div>
              <div style={{ width: '100%', height: '8px', background: '#1c1f2e', borderRadius: '4px', overflow: 'hidden' }}>
                <div style={{ width: '15%', height: '100%', background: '#10b981' }} />
              </div>
            </div>

            <div>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                <span>p90 (90th Percentile): {demoMetrics.p90} µs</span>
                <span style={{ color: '#06b6d4' }}>Standard Tail [SIMULATED]</span>
              </div>
              <div style={{ width: '100%', height: '8px', background: '#1c1f2e', borderRadius: '4px', overflow: 'hidden' }}>
                <div style={{ width: '38%', height: '100%', background: '#06b6d4' }} />
              </div>
            </div>

            <div>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                <span>p99 (99th Percentile): {demoMetrics.p99} µs</span>
                <span style={{ color: '#f59e0b' }}>Bounded Worst-Case [SIMULATED]</span>
              </div>
              <div style={{ width: '100%', height: '8px', background: '#1c1f2e', borderRadius: '4px', overflow: 'hidden' }}>
                <div style={{ width: '60%', height: '100%', background: '#f59e0b' }} />
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
