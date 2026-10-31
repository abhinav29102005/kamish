import React from 'react';

export default function TasksPage({ onSelectTask, onNewTask }) {
  const taskList = [
    { id: 'TSK-001', title: 'Segmented Commit Log Fsync Durability & Offset Sequential Reads', status: 'PASS' },
    { id: 'TSK-002', title: 'Session Manager Producer Sequence Deduplication (INV-6)', status: 'PASS' },
    { id: 'TSK-003', title: 'Bounded Durable Outbox & Destination Watermark Trimming', status: 'PASS' },
    { id: 'TSK-004', title: 'Single Decision Function Mapping E1 to E10 Enforcement Points', status: 'PASS' },
    { id: 'TSK-005', title: 'Sealed Destination Mode & Regional Key Residency Service', status: 'PASS' },
    { id: 'TSK-006', title: 'Dynamic WAN Route Selection with Hysteresis & Dwell Time', status: 'PASS' },
    { id: 'TSK-007', title: 'Partition Ownership, Lease Expiry, Fencing, and Quarantine Store', status: 'PASS' },
    { id: 'TSK-008', title: 'Zero-Payload Cryptographic SHA-256 Hash Chained Audit Log', status: 'PASS' },
    { id: 'TSK-009', title: 'Canary Scanner, Ordering Checker, and Epoch Auditor', status: 'PASS' },
    { id: 'TSK-010', title: 'Sustained Throughput Benchmark (12,479 msg/s & 59µs p50 Latency)', status: 'PASS' },
  ];

  return (
    <div
      style={{
        maxWidth: '1240px',
        margin: '0 auto',
        padding: '2rem 3.5rem 120px',
      }}
    >
      {/* Header matching Page 3: TASKS + NEW TASK */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '2.5rem',
        }}
      >
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
          TASKS
        </h1>

        <button
          onClick={onNewTask}
          className="brutal-btn-primary"
          style={{
            padding: '0.75rem 1.75rem',
            fontSize: '0.95rem',
            fontWeight: 800,
          }}
        >
          + NEW TASK
        </button>
      </div>

      {/* Task Rows matching Page 3 */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
        {taskList.map((task) => (
          <div
            key={task.id}
            onClick={() => onSelectTask(task.id)}
            style={{
              background: '#120b22',
              border: '2px solid #ffffff',
              boxShadow: '6px 6px 0px var(--purple-glow)',
              padding: '1.25rem 2rem',
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              cursor: 'pointer',
              transition: 'all 0.15s ease',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.transform = 'translate(-2px, -2px)';
              e.currentTarget.style.boxShadow = '8px 8px 0px var(--purple-accent)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.transform = 'translate(0px, 0px)';
              e.currentTarget.style.boxShadow = '6px 6px 0px var(--purple-glow)';
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '2rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '1rem',
                  fontWeight: 700,
                  color: '#ffffff',
                  minWidth: '90px',
                }}
              >
                {task.id}
              </span>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: '#e2d9f3',
                }}
              >
                [{task.title}]
              </span>
            </div>

            <div style={{ display: 'flex', alignItems: 'center', gap: '1.5rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.9rem',
                  color: '#ffffff',
                  fontWeight: 700,
                  border: '1px solid #ffffff',
                  padding: '3px 10px',
                  borderRadius: '2px',
                }}
              >
                [{task.status}]
              </span>
              <span style={{ fontSize: '1.2rem', color: '#c084fc' }}>→</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
