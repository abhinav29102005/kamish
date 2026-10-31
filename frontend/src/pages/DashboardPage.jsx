import React from 'react';

export default function DashboardPage({ onSelectTask }) {
  const stats = [
    { label: 'ACTIVE', count: '03', path: 'M0,15 Q30,5 60,12 T120,8 T180,14 T240,6' },
    { label: 'DONE', count: '21', path: 'M0,18 Q40,16 80,12 T160,8 T240,2' },
    { label: 'BLOCKED', count: '01', path: 'M0,10 Q50,15 100,8 T200,12 T240,14' },
    { label: 'OVERDUE', count: '00', path: 'M0,14 Q60,12 120,16 T180,11 T240,15' },
  ];

  const recentTasks = [
    { id: 'TSK-021', title: 'Sustained Throughput Benchmark', status: 'PASS' },
    { id: 'TSK-020', title: 'Client SDK & Administrative CLI', status: 'PASS' },
    { id: 'TSK-019', title: 'Ingress and Egress Interceptor Hooks', status: 'PASS' },
    { id: 'TSK-018', title: 'Fsync Durability & Commit Log Replay', status: 'PASS' },
  ];

  return (
    <div
      style={{
        maxWidth: '1240px',
        margin: '0 auto',
        padding: '2rem 3.5rem 120px',
      }}
    >
      {/* Title matching Page 2: DASHBOARD */}
      <h1
        style={{
          fontFamily: 'var(--font-main)',
          fontSize: '3.5rem',
          fontWeight: 900,
          letterSpacing: '0.04em',
          color: '#ffffff',
          marginBottom: '2rem',
          textTransform: 'uppercase',
        }}
      >
        DASHBOARD
      </h1>

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
        {/* Left: THROUGHPUT · SAMPLE DATA Chart Box */}
        <div className="brutal-card-offset" style={{ padding: '2rem' }}>
          <div
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '0.9rem',
              letterSpacing: '0.16em',
              color: '#d8b4fe',
              marginBottom: '2rem',
              fontWeight: 600,
            }}
          >
            THROUGHPUT · SAMPLE DATA (12,479 MSG/S)
          </div>

          <svg width="100%" height="220" viewBox="0 0 540 220" fill="none">
            {/* Grid lines */}
            <line x1="0" y1="30" x2="540" y2="30" stroke="rgba(255, 255, 255, 0.12)" strokeWidth="1" />
            <line x1="0" y1="80" x2="540" y2="80" stroke="rgba(255, 255, 255, 0.12)" strokeWidth="1" />
            <line x1="0" y1="130" x2="540" y2="130" stroke="rgba(255, 255, 255, 0.12)" strokeWidth="1" />
            <line x1="0" y1="180" x2="540" y2="180" stroke="rgba(255, 255, 255, 0.12)" strokeWidth="1" />

            {/* Solid Purple Line */}
            <path
              d="M0,170 Q45,150 90,165 T180,105 T270,135 T360,75 T450,115 T540,85"
              stroke="#c084fc"
              strokeWidth="3.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />

            {/* Dashed White Line */}
            <path
              d="M0,195 Q50,185 100,190 T200,150 T300,170 T400,135 T500,120 T540,130"
              stroke="#ffffff"
              strokeWidth="2.5"
              strokeDasharray="6 6"
              strokeLinecap="round"
            />
          </svg>
        </div>

        {/* Right: RECENT TASKS Box */}
        <div className="brutal-card-offset" style={{ padding: '2rem' }}>
          <div
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '0.9rem',
              letterSpacing: '0.16em',
              color: '#d8b4fe',
              marginBottom: '1.5rem',
              fontWeight: 600,
            }}
          >
            RECENT TASKS
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
            {recentTasks.map((t) => (
              <div
                key={t.id}
                onClick={() => onSelectTask(t.id)}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  borderBottom: '1px solid rgba(255, 255, 255, 0.12)',
                  paddingBottom: '0.85rem',
                  cursor: 'pointer',
                }}
              >
                <div>
                  <span
                    style={{
                      fontFamily: 'var(--font-mono)',
                      fontSize: '0.95rem',
                      color: '#ffffff',
                      fontWeight: 600,
                      marginRight: '8px',
                    }}
                  >
                    {t.id}
                  </span>
                  <span style={{ fontSize: '0.9rem', color: '#c4b5fd' }}>{t.title}</span>
                </div>
                <span
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: '0.82rem',
                    color: '#34d399',
                    fontWeight: 700,
                  }}
                >
                  [{t.status}]
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
