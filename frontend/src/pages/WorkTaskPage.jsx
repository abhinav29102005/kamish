import React from 'react';

export default function WorkTaskPage({ taskId = 'TSK-019', onBack }) {
  const details = {
    id: taskId,
    title: 'Ingress and Egress Interceptor Hooks Calling PolicyEngine',
    status: 'PASS',
    owner: 'Developer A & B (Systems Integration)',
    description:
      'Integrated PolicyEngine trait contract into BrokerServer. Ingress hook validates producer rules before disk append; egress hook blocks StrictLocal records from remote replication outboxes, preserving regional data residency under INV-8 with zero-payload cryptographic audit logging.',
    activities: [
      { timestamp: '18:02:14 UTC', event: 'Ingress interceptor hook validated payload in EU_WEST' },
      { timestamp: '18:02:15 UTC', event: 'Hardware fsync flush confirmed on segment 0000.log' },
      { timestamp: '18:02:16 UTC', event: 'Egress replication to US_EAST strictly blocked (StrictLocal)' },
      { timestamp: '18:02:17 UTC', event: 'SHA-256 zero-payload audit entry #104 committed to chain' },
    ],
  };

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
            fontSize: '3.5rem',
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
      <div
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
                  minWidth: '80px',
                  fontWeight: 600,
                }}
              >
                TITLE
              </span>
              <span style={{ fontFamily: 'var(--font-mono)', fontSize: '1rem', color: '#ffffff', fontWeight: 600 }}>
                [{details.title}]
              </span>
            </div>

            <div style={{ display: 'flex', gap: '2rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: 'var(--text-dim)',
                  minWidth: '80px',
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
                [{details.status}]
              </span>
            </div>

            <div style={{ display: 'flex', gap: '2rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: 'var(--text-dim)',
                  minWidth: '80px',
                  fontWeight: 600,
                }}
              >
                OWNER
              </span>
              <span style={{ fontFamily: 'var(--font-mono)', fontSize: '0.95rem', color: '#ffffff' }}>
                [{details.owner}]
              </span>
            </div>

            <div style={{ marginTop: '1rem', borderTop: '1px solid rgba(255, 255, 255, 0.1)', paddingTop: '1.5rem' }}>
              <p
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.92rem',
                  lineHeight: 1.7,
                  color: '#d8b4fe',
                }}
              >
                [{details.description}]
              </p>
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
            ACTIVITY
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '1.5rem' }}>
            {details.activities.map((act, i) => (
              <div key={i} style={{ borderBottom: '1px solid rgba(255, 255, 255, 0.08)', paddingBottom: '1rem' }}>
                <span
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: '0.78rem',
                    color: '#c084fc',
                    display: 'block',
                    marginBottom: '4px',
                  }}
                >
                  [{act.timestamp}]
                </span>
                <span
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: '0.88rem',
                    color: '#f8fafc',
                  }}
                >
                  [{act.event}]
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
