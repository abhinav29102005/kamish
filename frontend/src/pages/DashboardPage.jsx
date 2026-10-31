import React, { useState } from 'react';
import { useKamish } from '../context/KamishContext';

export default function DashboardPage({ onSelectTask, onNavigate }) {
  const {
    nodes,
    auditLedger,
    telemetryHistory,
    currentTelemetry,
    produceCustomEvent,
    verifyAuditLedgerIntegrity,
    runScenario,
    scenarioState,
  } = useKamish();

  const [verifyResult, setVerifyResult] = useState(null);
  const [isVerifying, setIsVerifying] = useState(false);

  const activeNodeCount = Object.values(nodes).filter((n) => n.status === 'HEALTHY').length;
  const acceptedCount = auditLedger.filter((a) => a.outcome === 'Accept').length;
  const blockedCount = auditLedger.filter((a) => a.outcome === 'Deny').length;

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
    await produceCustomEvent(
      'kamish-orders',
      `ORD-${Math.floor(Math.random() * 90000 + 10000)}`,
      'EU_GDPR_STRICT'
    );
  };

  // Generate dynamic SVG path from telemetryHistory
  const samples = telemetryHistory.slice(-12);
  const maxThroughput = Math.max(...samples.map((s) => s.throughputEps), 15000);
  const svgWidth = 540;
  const svgHeight = 220;

  const points = samples.map((s, idx) => {
    const x = (idx / (samples.length - 1 || 1)) * svgWidth;
    const y = svgHeight - (s.throughputEps / maxThroughput) * (svgHeight - 40) - 20;
    return `${x},${y}`;
  });

  const pathD = points.length > 1
    ? `M${points[0]} ` + points.slice(1).map((p) => `L${p}`).join(' ')
    : 'M0,170 Q45,150 90,165 T180,105 T270,135 T360,75 T450,115 T540,85';

  const stats = [
    { label: 'ACTIVE', count: `0${activeNodeCount}`, path: 'M0,15 Q30,5 60,12 T120,8 T180,14 T240,6' },
    { label: 'DONE', count: acceptedCount > 9 ? String(acceptedCount) : `0${acceptedCount}`, path: 'M0,18 Q40,16 80,12 T160,8 T240,2' },
    { label: 'BLOCKED', count: blockedCount > 9 ? String(blockedCount) : `0${blockedCount}`, path: 'M0,10 Q50,15 100,8 T200,12 T240,14' },
    { label: 'OVERDUE', count: '00', path: 'M0,14 Q60,12 120,16 T180,11 T240,15' },
  ];

  return (
    <div
      style={{
        maxWidth: '1240px',
        margin: '0 auto',
        padding: '2rem 3.5rem 120px',
      }}
    >
      {/* Header bar matching Mockup Page 2 */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '2rem', flexWrap: 'wrap', gap: '1rem' }}>
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

        {/* Action Controls */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', flexWrap: 'wrap' }}>
          <button
            onClick={handleQuickProduce}
            className="brutal-btn-primary"
            style={{ padding: '0.75rem 1.75rem', fontSize: '0.85rem' }}
          >
            + PRODUCE EVENT
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
            {isVerifying ? 'VERIFYING...' : 'VERIFY SHA-256 AUDIT'}
          </button>
        </div>
      </div>

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
            ? `✓ Cryptographic Audit Chain Validated: ${verifyResult.count} consecutive blocks sealed with unbroken SHA-256 hashes (Genesis -> Tip).`
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
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1.7fr 1fr',
          gap: '2rem',
        }}
      >
        {/* Left: THROUGHPUT LIVE BROKER TELEMETRY */}
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
              THROUGHPUT · LIVE STREAM ({(currentTelemetry?.throughputEps || 12479).toLocaleString()} MSG/S)
            </div>
            <span
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.78rem',
                color: 'var(--text-dim)',
              }}
            >
              P99: {(currentTelemetry?.p99LatencyMs || 0.4).toFixed(1)}ms
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

        {/* Right: RECENT TASKS / AUDIT LOG */}
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
            <span>RECENT TASKS</span>
            <span
              onClick={() => onNavigate('tasks')}
              style={{
                fontSize: '0.75rem',
                color: '#c084fc',
                cursor: 'pointer',
                textDecoration: 'underline',
              }}
            >
              ALL TASKS →
            </span>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
            {auditLedger.slice(-4).reverse().map((entry) => (
              <div
                key={entry.id}
                onClick={() => onSelectTask(entry.key)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.88rem',
                  borderBottom: '1px solid rgba(255, 255, 255, 0.08)',
                  paddingBottom: '0.75rem',
                  cursor: 'pointer',
                  transition: 'background 0.15s ease',
                }}
              >
                <div>
                  <span style={{ color: '#ffffff', fontWeight: 700, marginRight: '8px' }}>
                    {entry.key}
                  </span>
                  <span style={{ color: 'var(--text-dim)' }}>
                    {entry.topic}
                  </span>
                </div>
                <span
                  style={{
                    color: entry.outcome === 'Accept' ? '#4ade80' : '#f87171',
                    fontWeight: 700,
                  }}
                >
                  [{entry.outcome === 'Accept' ? 'PASS' : 'BLOCKED'}]
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
