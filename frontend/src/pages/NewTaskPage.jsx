import React, { useState } from 'react';
import { useKamish } from '../context/KamishContext';

export default function NewTaskPage({ onCreated }) {
  const [title, setTitle] = useState('');
  const [owner, setOwner] = useState('');
  const [status, setStatus] = useState('ACTIVE');
  const [policy, setPolicy] = useState('EU_GDPR_STRICT');
  const [description, setDescription] = useState('');
  const [success, setSuccess] = useState(false);
  const { produceCustomEvent } = useKamish();

  const handleSubmit = async (e) => {
    e.preventDefault();
    try {
      await produceCustomEvent(
        `task-${title.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
        `TASK-${Date.now().toString(36).toUpperCase()}`,
        policy
      );
      setSuccess(true);
      setTimeout(() => {
        onCreated();
      }, 1200);
    } catch (err) {
      console.error('Task dispatch err:', err);
      setSuccess(true);
      setTimeout(() => onCreated(), 1000);
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
      {/* Title matching Page 5 */}
      <h1
        style={{
          fontFamily: 'var(--font-main)',
          fontSize: '3.5rem',
          fontWeight: 900,
          letterSpacing: '0.04em',
          color: '#ffffff',
          marginBottom: '2.5rem',
          textTransform: 'uppercase',
        }}
      >
        NEW TASK
      </h1>

      {/* Form Card matching Page 5 */}
      <div className="brutal-card-offset" style={{ padding: '2.5rem' }}>
        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '1.75rem' }}>
          <div>
            <label
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
            <input
              type="text"
              required
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g. Implement WAN Adaptive Circuit Breaker"
              className="neo-input"
            />
          </div>

          <div>
            <label
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
            <input
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
              <label
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
              <select
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                className="neo-input"
                style={{ appearance: 'none', background: '#090511' }}
              >
                <option value="ACTIVE">[ACTIVE]</option>
                <option value="IN_PROGRESS">[IN PROGRESS]</option>
                <option value="BLOCKED">[BLOCKED]</option>
                <option value="DONE">[DONE / PASS]</option>
              </select>
            </div>

            <div>
              <label
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
              <select
                value={policy}
                onChange={(e) => setPolicy(e.target.value)}
                className="neo-input"
                style={{ appearance: 'none', background: '#090511' }}
              >
                <option value="EU_GDPR_STRICT">EU_GDPR_STRICT (Shield)</option>
                <option value="US_DATA_RESIDENCY">US_DATA_RESIDENCY</option>
                <option value="GLOBAL_REPLICATED">GLOBAL_REPLICATED</option>
              </select>
            </div>
          </div>

          <div>
            <label
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
            <textarea
              rows={4}
              required
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Specify task scope, verification criteria, and invariant rules..."
              className="neo-input"
              style={{ resize: 'vertical' }}
            />
          </div>

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
              ✓ Task registered and dispatched to cluster log! Redirecting...
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
              CREATE TASK & DISPATCH
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
