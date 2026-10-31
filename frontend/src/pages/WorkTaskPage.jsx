import React, { useState } from 'react';
import { useKamish } from '../context/useKamish';
import { useDemoTasks } from '../context/useDemoTasks.js';

export default function WorkTaskPage({ taskId = 'TSK-001', onBack }) {
  const { auditLedger, verifyAuditLedgerIntegrity } = useKamish();
  const { tasks } = useDemoTasks();
  const [isRunningCheck, setIsRunningCheck] = useState(false);
  const [testResult, setTestResult] = useState(null);

  const meta = tasks.find(task => task.id === taskId);

  const handleRunVerification = async () => {
    setIsRunningCheck(true);
    try {
      const res = await verifyAuditLedgerIntegrity();
      setTestResult({
        success: res.valid,
        message: res.valid
          ? `Local demo audit: ${res.count} hash-chain entries checked. No hardware, broker, or residency verification was performed.`
          : `Local demo audit check failed: ${res.error}`,
      });
    } catch (err) {
      setTestResult({ success: false, message: `Error: ${String(err)}` });
    } finally {
      setIsRunningCheck(false);
    }
  };

  const recentAudits = auditLedger.slice(-4).reverse();

  if (!meta) return <section style={{ padding: '2rem' }}>
    <h1>TASK NOT FOUND</h1>
    <p>No stored demo task exists for this ID. No verification status is available.</p>
    <button className="brutal-btn-primary" onClick={onBack}>BACK TO TASKS</button>
  </section>;

  return (
    <div
      style={{
        maxWidth: '1240px',
        margin: '0 auto',
        padding: '2rem 3.5rem 120px',
      }}
    >
      {/* Route Subtitle & Title matching Page 4 */}
      <div style={{ marginBottom: '2.5rem' }}>
        <button
          onClick={onBack}
          style={{
            background: 'transparent',
            border: 'none',
            color: '#c084fc',
            fontFamily: 'var(--font-mono)',
            fontSize: '0.9rem',
            cursor: 'pointer',
            marginBottom: '0.75rem',
            display: 'inline-flex',
            alignItems: 'center',
            gap: '6px',
          }}
        >
          ← BACK TO TASKS
        </button>

        <div
          style={{
            fontFamily: 'var(--font-mono)',
            fontSize: '0.9rem',
            color: 'var(--text-dim)',
            letterSpacing: '0.08em',
            marginBottom: '0.5rem',
          }}
        >
          /work-{taskId.toLowerCase()}-task
        </div>

        <h1
          style={{
            fontFamily: 'var(--font-main)',
            fontSize: 'clamp(2rem, 4vw, 3.5rem)',
            overflowWrap: 'anywhere',
            fontWeight: 900,
            letterSpacing: '0.04em',
            color: '#ffffff',
            textTransform: 'uppercase',
          }}
        >
          WORK-{taskId.toUpperCase()} TASK
        </h1>
      </div>

      {/* Main Grid: Left Task Details + Right Activity Box */}
      <div className="demo-main-grid"
        style={{
          display: 'grid',
          gridTemplateColumns: '1.7fr 1.1fr',
          gap: '2.5rem',
        }}
      >
        {/* Left: Task Detail Box matching Page 4 */}
        <div className="brutal-card-offset" style={{ padding: '2.5rem' }}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '1.75rem' }}>
            <div style={{ display: 'flex', gap: '2rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: 'var(--text-dim)',
                  minWidth: '90px',
                  fontWeight: 600,
                }}
              >
                TITLE
              </span>
              <span style={{ fontFamily: 'var(--font-mono)', fontSize: '1rem', color: '#ffffff', fontWeight: 600 }}>
                [{meta.title}]
              </span>
            </div>

            <div style={{ display: 'flex', gap: '2rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: 'var(--text-dim)',
                  minWidth: '90px',
                  fontWeight: 600,
                }}
              >
                STATUS
              </span>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: '#34d399',
                  fontWeight: 700,
                  border: '1px solid #34d399',
                  padding: '2px 10px',
                  borderRadius: '2px',
                }}
              >
                [{meta.status} · DEMO]
              </span>
            </div>

            <div style={{ display: 'flex', gap: '2rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: 'var(--text-dim)',
                  minWidth: '90px',
                  fontWeight: 600,
                }}
              >
                OWNER
              </span>
              <span style={{ fontFamily: 'var(--font-mono)', fontSize: '0.95rem', color: '#ffffff' }}>
                [{meta.owner}]
              </span>
            </div>

            <div style={{ display: 'flex', gap: '2rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: 'var(--text-dim)',
                  minWidth: '90px',
                  fontWeight: 600,
                }}
              >
                INVARIANT
              </span>
              <span style={{ fontFamily: 'var(--font-mono)', fontSize: '0.95rem', color: '#c084fc', fontWeight: 700 }}>
                [{meta.rule}]
              </span>
            </div>

            <div style={{ marginTop: '0.5rem', borderTop: '1px solid rgba(255, 255, 255, 0.1)', paddingTop: '1.5rem' }}>
              <p
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.92rem',
                  lineHeight: 1.7,
                  color: '#d8b4fe',
                }}
              >
                [{meta.description}]
              </p>
            </div>

            {testResult && (
              <div
                style={{
                  padding: '1rem',
                  background: testResult.success ? 'rgba(52, 211, 153, 0.12)' : 'rgba(239, 68, 68, 0.12)',
                  border: `1px solid ${testResult.success ? '#34d399' : '#ef4444'}`,
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.85rem',
                  color: testResult.success ? '#86efac' : '#fca5a5',
                }}
              >
                {testResult.message}
              </div>
            )}

            <div style={{ marginTop: '0.5rem' }}>
              <button
                onClick={handleRunVerification}
                disabled={isRunningCheck}
                className="brutal-btn-primary"
                style={{
                  padding: '0.85rem 2.2rem',
                  fontSize: '0.95rem',
                  fontWeight: 800,
                }}
              >
                {isRunningCheck ? 'CHECKING LOCAL AUDIT...' : 'CHECK LOCAL DEMO AUDIT'}
              </button>
            </div>
          </div>
        </div>

        {/* Right: ACTIVITY Box matching Page 4 */}
        <div
          className="stat-card-rounded"
          style={{
            padding: '2rem',
            border: '1px solid rgba(168, 85, 247, 0.35)',
            background: 'rgba(18, 11, 30, 0.85)',
          }}
        >
          <div
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '0.85rem',
              letterSpacing: '0.14em',
              color: 'var(--text-dim)',
              marginBottom: '2rem',
              fontWeight: 700,
            }}
          >
            SIMULATED AUDIT ACTIVITY
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '1.5rem' }}>
            {recentAudits.map((act) => (
              <div key={act.seq} style={{ borderBottom: '1px solid rgba(255, 255, 255, 0.08)', paddingBottom: '1rem' }}>
                <span
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: '0.78rem',
                    color: '#c084fc',
                    display: 'block',
                    marginBottom: '4px',
                  }}
                >
                  [{new Date(act.timestamp).toISOString()}]
                </span>
                <span
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: '0.85rem',
                    color: '#f8fafc',
                  }}
                >
                  [#{act.seq} · {act.topic} · {act.sourceRegion} → {act.targetRegion} · {act.outcome === 'Deny' ? 'Denied' : 'Allowed'} (demo)]
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
