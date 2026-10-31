import React, { useState } from 'react';
import { useDemoTasks } from '../context/useDemoTasks.js';

export default function NewTaskPage({ onCreated }) {
  const [title, setTitle] = useState('');
  const [owner, setOwner] = useState('');
  const [status, setStatus] = useState('ACTIVE');
  const [policy, setPolicy] = useState('EU_GDPR_STRICT');
  const [description, setDescription] = useState('');
  const [success, setSuccess] = useState(false);
  const [error, setError] = useState(null);
  const { createDemoTask } = useDemoTasks();

  const handleSubmit = async (e) => {
    e.preventDefault();
    setError(null);
    try {
      const task = createDemoTask({ title, owner, status, policy, description });
      setSuccess(true);
      onCreated(task);
    } catch (err) {
      setSuccess(false);
      setError(err instanceof Error ? err.message : 'Could not save demo task');
    }
  };

  return (
    <div
      style={{
        maxWidth: '720px',
        margin: '0 auto',
        padding: '2rem 2rem 120px',
      }}
    >
      {/* Title matching Page 5 with Logo */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '1.25rem', marginBottom: '2.5rem' }}>
        <img
          src="/logo.png"
          alt="Kamish Dragon Logo"
          style={{
            width: '56px',
            height: '56px',
            objectFit: 'contain',
            filter: 'drop-shadow(0 0 12px rgba(192, 132, 252, 0.7))',
          }}
        />
        <h1
          style={{
            fontFamily: 'var(--font-main)',
            fontSize: '3.5rem',
            fontWeight: 900,
            letterSpacing: '0.04em',
            color: '#ffffff',
            margin: 0,
            textTransform: 'uppercase',
          }}
        >
          NEW TASK
        </h1>
      </div>

      {/* Form Card matching Page 5 */}
      <div className="brutal-card-offset" style={{ padding: '2.5rem' }}>
        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '1.75rem' }}>
          <div>
            <label htmlFor="task-title"
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.8rem',
                color: 'var(--text-dim)',
                letterSpacing: '0.12em',
                display: 'block',
                marginBottom: '0.6rem',
                fontWeight: 600,
              }}
            >
              TITLE
            </label>
            <input id="task-title"
              type="text"
              required
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g. Implement WAN Adaptive Circuit Breaker"
              className="neo-input"
            />
          </div>

          <div>
            <label htmlFor="task-owner"
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.8rem',
                color: 'var(--text-dim)',
                letterSpacing: '0.12em',
                display: 'block',
                marginBottom: '0.6rem',
                fontWeight: 600,
              }}
            >
              OWNER / LEAD
            </label>
            <input id="task-owner"
              type="text"
              required
              value={owner}
              onChange={(e) => setOwner(e.target.value)}
              placeholder="e.g. Developer B (Reliability Lead)"
              className="neo-input"
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1.5rem' }}>
            <div>
              <label htmlFor="task-status"
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.8rem',
                  color: 'var(--text-dim)',
                  letterSpacing: '0.12em',
                  display: 'block',
                  marginBottom: '0.6rem',
                  fontWeight: 600,
                }}
              >
                STATUS
              </label>
              <select id="task-status"
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                className="neo-input"
                style={{ appearance: 'none', background: '#090511' }}
              >
                <option value="ACTIVE">[ACTIVE]</option>
                <option value="IN_PROGRESS">[IN PROGRESS]</option>
                <option value="BLOCKED">[BLOCKED]</option>
                <option value="DONE">[DONE — DEMO]</option>
              </select>
            </div>

            <div>
              <label htmlFor="task-policy"
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.8rem',
                  color: 'var(--text-dim)',
                  letterSpacing: '0.12em',
                  display: 'block',
                  marginBottom: '0.6rem',
                  fontWeight: 600,
                }}
              >
                RESIDENCY POLICY
              </label>
              <select id="task-policy"
                value={policy}
                onChange={(e) => setPolicy(e.target.value)}
                className="neo-input"
                style={{ appearance: 'none', background: '#090511' }}
              >
                <option value="EU_GDPR_STRICT">EU LOCAL ONLY (DEMO)</option>
                <option value="US_DATA_RESIDENCY">US DESTINATION (DEMO)</option>
                <option value="GLOBAL_REPLICATED">ALL DEMO REGIONS</option>
              </select>
            </div>
          </div>

          <div>
            <label htmlFor="task-description"
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.8rem',
                color: 'var(--text-dim)',
                letterSpacing: '0.12em',
                display: 'block',
                marginBottom: '0.6rem',
                fontWeight: 600,
              }}
            >
              DESCRIPTION & SCOPE
            </label>
            <textarea id="task-description"
              rows={4}
              required
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Specify task scope, verification criteria, and invariant rules..."
              className="neo-input"
              style={{ resize: 'vertical' }}
            />
          </div>

          {error && <p role="alert">{error}</p>}
          {success && (
            <div
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.85rem',
                color: '#34d399',
                border: '1px solid #34d399',
                padding: '8px 12px',
                background: 'rgba(52, 211, 153, 0.1)',
              }}
            >
              ✓ Demo task saved in this browser. No broker dispatch was performed.
            </div>
          )}

          <div style={{ marginTop: '0.5rem' }}>
            <button
              type="submit"
              className="brutal-btn-primary"
              style={{
                padding: '0.85rem 2.2rem',
                fontSize: '0.95rem',
                fontWeight: 800,
              }}
            >
              SAVE DEMO TASK LOCALLY
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
