import React, { useState } from 'react';
import { useKamish } from '../context/useKamish';
import { policyForSelection } from '../utils/demoPolicy.js';

export default function DashboardPage({ onNavigate }) {
  const {
    nodes,
    auditLedger,
    auditError,
    telemetryHistory,
    currentTelemetry,
    produceCustomEvent,
    verifyAuditLedgerIntegrity,
    isLiveMode,
    connectionStatus,
    gatewayStatus,
  } = useKamish();

  const [verifyResult, setVerifyResult] = useState(null);
  const [isVerifying, setIsVerifying] = useState(false);
  const [produceError, setProduceError] = useState(null);

  const activeNodeCount = Object.values(nodes).filter((n) => n.status === 'HEALTHY').length;
  const acceptedCount = auditLedger.filter((a) => a.outcome === 'AllowCleartext' || a.outcome === 'AllowSealed').length;
  const blockedCount = auditLedger.filter((a) => a.outcome === 'Deny').length;
  const liveConnected = isLiveMode && connectionStatus === 'connected';

  const handleVerifyAudit = async () => {
    setIsVerifying(true);
    try {
      const res = await verifyAuditLedgerIntegrity();
      setVerifyResult(res);
      setTimeout(() => setVerifyResult(null), 5000);
    } catch (e) {
      setVerifyResult({ valid: false, count: 0, error: String(e) });
    } finally {
      setIsVerifying(false);
    }
  };

  const handleQuickProduce = async () => {
    setProduceError(null);
    try {
      await produceCustomEvent('kamish-orders', `ORD-${Date.now()}`, policyForSelection('EU_GDPR_STRICT'));
    } catch (error) {
      setProduceError(error instanceof Error ? error.message : 'Could not produce event');
    }
  };

  // Dynamic SVG path from telemetryHistory
  const samples = telemetryHistory.slice(-12);
  const maxThroughput = Math.max(...samples.map((s) => s.throughputEps), 1);
  const svgWidth = 540;
  const svgHeight = 220;

  const points = samples.map((s, idx) => {
    const x = (idx / (samples.length - 1 || 1)) * svgWidth;
    const y = svgHeight - (s.throughputEps / maxThroughput) * (svgHeight - 40) - 20;
    return `${x},${y}`;
  });

  const pathD = isLiveMode
    ? ''
    : points.length > 1
      ? `M${points[0]} ` + points.slice(1).map((p) => `L${p}`).join(' ')
      : 'M0,170 Q45,150 90,165 T180,105 T270,135 T360,75 T450,115 T540,85';

  const stats = isLiveMode ? [
    {
      label: 'CONNECTED GATEWAY REGION',
      count: liveConnected ? (gatewayStatus?.region || '—') : '—',
      path: 'M0,15 Q30,5 60,12 T120,8 T180,14 T240,6',
    },
    {
      label: 'ACCEPTED DECISIONS',
      count: 'N/A',
      path: 'M0,18 Q40,16 80,12 T160,8 T240,2',
    },
    {
      label: 'DENIED DECISIONS',
      count: 'N/A',
      path: 'M0,10 Q50,15 100,8 T200,12 T240,14',
    },
    {
      label: 'GATEWAY OUTBOX PENDING',
      count: liveConnected ? String(currentTelemetry?.outboxDepth ?? 0) : '—',
      path: 'M0,14 Q60,12 120,16 T180,11 T240,15',
    },
  ] : [
    {
      label: 'HEALTHY DEMO REGIONS',
      count: String(activeNodeCount).padStart(2, '0'),
      path: 'M0,15 Q30,5 60,12 T120,8 T180,14 T240,6',
    },
    {
      label: 'ALLOWED DEMO DECISIONS',
      count: String(acceptedCount).padStart(2, '0'),
      path: 'M0,18 Q40,16 80,12 T160,8 T240,2',
    },
    {
      label: 'DENIED DEMO DECISIONS',
      count: String(blockedCount).padStart(2, '0'),
      path: 'M0,10 Q50,15 100,8 T200,12 T240,14',
    },
    {
      label: 'SIMULATED QUEUE DEPTH',
      count: String(nodes.EU_WEST.outboxQueueSize).padStart(2, '0'),
      path: 'M0,14 Q60,12 120,16 T180,11 T240,15',
    },
  ];

  return (
    <div
      style={{
        maxWidth: '1240px',
        margin: '0 auto',
        padding: '2rem 3.5rem 120px',
      }}
    >
      {/* Header bar */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '2rem', flexWrap: 'wrap', gap: '1rem' }}>
        <div>
          <h1
            style={{
              fontFamily: 'var(--font-main)',
              fontSize: '3.5rem',
              fontWeight: 900,
              letterSpacing: '0.04em',
              color: '#ffffff',
              textTransform: 'uppercase',
              margin: 0,
            }}
          >
            DASHBOARD
          </h1>
          <div style={{ fontFamily: 'var(--font-mono)', fontSize: '0.85rem', color: isLiveMode ? '#4ade80' : '#facc15', marginTop: '0.35rem' }}>
            {isLiveMode ? `MODE: LIVE BROKER GATEWAY (${connectionStatus.toUpperCase()})` : 'MODE: LOCAL BROWSER DEMO SIMULATION'}
          </div>
        </div>

        {/* Action Controls */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', flexWrap: 'wrap' }}>
          <button
            onClick={handleQuickProduce}
            className="brutal-btn-primary"
            style={{ padding: '0.75rem 1.75rem', fontSize: '0.85rem' }}
          >
            {isLiveMode ? '+ PRODUCE BROKER EVENT' : '+ SIMULATE EVENT'}
          </button>

          <button
            onClick={handleVerifyAudit}
            disabled={isVerifying}
            className="brutal-btn-white"
            style={{
              padding: '0.75rem 1.75rem',
              fontSize: '0.85rem',
              cursor: isVerifying ? 'wait' : 'pointer',
            }}
          >
            {isVerifying ? (isLiveMode ? 'VERIFYING GATEWAY...' : 'CHECKING...') : (isLiveMode ? 'VERIFY GATEWAY AUDIT' : 'CHECK LOCAL DEMO AUDIT')}
          </button>
        </div>
      </div>

      {auditError && <p role="alert" style={{ color: '#fca5a5', fontFamily: 'var(--font-mono)', margin: '1rem 0' }}>Audit warning: {auditError}</p>}
      {produceError && <p role="alert" style={{ color: '#fca5a5', fontFamily: 'var(--font-mono)', margin: '1rem 0' }}>{produceError}</p>}

      {/* Verification notification banner */}
      {verifyResult && (
        <div
          style={{
            padding: '1rem 1.5rem',
            marginBottom: '2rem',
            background: verifyResult.valid ? 'rgba(34, 197, 94, 0.15)' : 'rgba(239, 68, 68, 0.15)',
            border: `1.5px solid ${verifyResult.valid ? '#22c55e' : '#ef4444'}`,
            borderRadius: '4px',
            fontFamily: 'var(--font-mono)',
            fontSize: '0.88rem',
            color: verifyResult.valid ? '#86efac' : '#fca5a5',
          }}
        >
          {verifyResult.valid
            ? isLiveMode
              ? `✓ Gateway cryptographic audit chain verified: ${verifyResult.count} entries authenticated.`
              : `✓ Local demo audit hashes validated: ${verifyResult.count} entries. No broker or hardware verification was performed.`
            : `✗ Audit Chain Verification Failed: ${verifyResult.error}`}
        </div>
      )}

      {/* 4 Stat Cards */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))',
          gap: '1.25rem',
          marginBottom: '2.5rem',
        }}
      >
        {stats.map((s) => (
          <div key={s.label} className="stat-card-rounded">
            <span
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.85rem',
                color: 'var(--text-dim)',
                letterSpacing: '0.12em',
                fontWeight: 600,
                display: 'block',
                marginBottom: '0.5rem',
              }}
            >
              {s.label}
            </span>
            <div
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '3rem',
                fontWeight: 800,
                color: '#ffffff',
                lineHeight: 1,
                marginBottom: '1rem',
              }}
            >
              {s.count}
            </div>
            {/* Sparkline */}
            <svg width="100%" height="24" viewBox="0 0 240 24" fill="none">
              <path
                d={s.path}
                stroke="#c084fc"
                strokeWidth="2"
                strokeLinecap="round"
              />
            </svg>
          </div>
        ))}
      </div>

      {/* Main Grid: Throughput Chart + Recent Tasks */}
      <div className="demo-main-grid"
        style={{
          display: 'grid',
          gridTemplateColumns: '1.7fr 1fr',
          gap: '2rem',
        }}
      >
        {/* Left: THROUGHPUT TELEMETRY */}
        <div className="brutal-card-offset" style={{ padding: '2rem' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              marginBottom: '2rem',
            }}
          >
            <div
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.9rem',
                letterSpacing: '0.16em',
                color: '#d8b4fe',
                fontWeight: 600,
              }}
            >
              {isLiveMode ? 'LIVE THROUGHPUT METRIC NOT EXPOSED BY GATEWAY' : `SIMULATED THROUGHPUT (${(currentTelemetry?.throughputEps || 0).toLocaleString()} MSG/S)`}
            </div>
            <span
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.78rem',
                color: 'var(--text-dim)',
              }}
            >
              {isLiveMode ? 'P99: N/A' : `P99: ${(currentTelemetry?.p99LatencyMs || 0.4).toFixed(1)}ms`}
            </span>
          </div>

          <svg width="100%" height="220" viewBox="0 0 540 220" fill="none">
            {/* Grid lines */}
            <line x1="0" y1="30" x2="540" y2="30" stroke="rgba(255, 255, 255, 0.08)" strokeWidth="1" />
            <line x1="0" y1="80" x2="540" y2="80" stroke="rgba(255, 255, 255, 0.08)" strokeWidth="1" />
            <line x1="0" y1="130" x2="540" y2="130" stroke="rgba(255, 255, 255, 0.08)" strokeWidth="1" />
            <line x1="0" y1="180" x2="540" y2="180" stroke="rgba(255, 255, 255, 0.08)" strokeWidth="1" />

            {/* Dynamic Real-Time Throughput Line */}
            <path
              d={pathD}
              stroke="#c084fc"
              strokeWidth="3.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </div>

        {/* Right: AUDIT LOG */}
        <div className="brutal-card-offset" style={{ padding: '2rem' }}>
          <div
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '0.9rem',
              letterSpacing: '0.16em',
              color: '#d8b4fe',
              marginBottom: '1.5rem',
              fontWeight: 600,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
            }}
          >
            <span>{isLiveMode ? 'LIVE BROKER AUDIT LOG' : 'DEMO AUDIT EVENTS'}</span>
            <button
              onClick={() => onNavigate('tasks')}
              style={{
                fontSize: '0.75rem',
                color: '#c084fc',
                cursor: 'pointer',
                textDecoration: 'underline',
                background: 'transparent', border: 0, fontFamily: 'inherit',
              }}
            >
              ALL TASKS →
            </button>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
            {auditLedger.length === 0 ? (
              <div style={{ fontFamily: 'var(--font-mono)', fontSize: '0.85rem', color: '#64748b', fontStyle: 'italic' }}>
                {isLiveMode ? 'No broker audit records yet in session' : 'No demo events generated yet'}
              </div>
            ) : (
              auditLedger.slice(-4).reverse().map((entry) => (
                <div
                  key={`${entry.seq}-${entry.hash || entry.timestamp}`}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    fontFamily: 'var(--font-mono)',
                    fontSize: '0.88rem',
                    borderBottom: '1px solid rgba(255, 255, 255, 0.08)',
                    paddingBottom: '0.75rem',
                    cursor: 'default',
                    transition: 'background 0.15s ease',
                  }}
                >
                  <div>
                    <span style={{ color: '#ffffff', fontWeight: 700, marginRight: '8px' }}>
                      #{entry.seq}{' '}
                    </span>
                    <span style={{ color: 'var(--text-dim)' }}>
                      {entry.topic}
                    </span>
                  </div>
                  <span
                    style={{
                      color: entry.outcome === 'Deny' ? '#f87171' : (entry.outcome === 'AllowCleartext' || entry.outcome === 'AllowSealed') ? '#4ade80' : '#facc15',
                      fontWeight: 700,
                    }}
                  >
                    [{entry.outcome === 'Deny' ? 'DENIED' : (entry.outcome === 'AllowCleartext' || entry.outcome === 'AllowSealed') ? 'ALLOWED' : 'UNKNOWN'} · {isLiveMode ? 'LIVE' : 'DEMO'}]
                  </span>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
