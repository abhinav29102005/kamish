import React, { useState } from 'react';

export default function NewTaskPage({ onCreated }) {
  const [title, setTitle] = useState('');
  const [owner, setOwner] = useState('');
  const [status, setStatus] = useState('ACTIVE');
  const [description, setDescription] = useState('');
  const [success, setSuccess] = useState(false);

  const handleSubmit = (e) => {
    e.preventDefault();
    setSuccess(true);
    setTimeout(() => {
      onCreated();
    }, 1200);
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
              OWNER
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
              DESCRIPTION
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
              }}
            >
              Task successfully registered! Redirecting to Tasks...
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
              CREATE TASK
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
