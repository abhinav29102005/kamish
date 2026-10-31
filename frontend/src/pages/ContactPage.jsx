import React, { useState } from 'react';

export default function ContactPage() {
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [message, setMessage] = useState('');
  const [sent, setSent] = useState(false);

  const handleSubmit = (e) => {
    e.preventDefault();
    setSent(true);
    setTimeout(() => {
      setName('');
      setEmail('');
      setMessage('');
      setSent(false);
    }, 2500);
  };

  return (
    <div
      style={{
        maxWidth: '720px',
        margin: '0 auto',
        padding: '2rem 2rem 120px',
      }}
    >
      {/* Title matching Page 6 */}
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
        CONTACT
      </h1>

      {/* Form Card matching Page 6 */}
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
              NAME
            </label>
            <input
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Thomas Andre / Sung Jin-Woo"
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
              EMAIL
            </label>
            <input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="hunter@national-bureau.gov"
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
              MESSAGE
            </label>
            <textarea
              rows={5}
              required
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder="Inquire regarding Kamish's Wrath daggers, sovereign cluster federation, or S-Rank raid logs..."
              className="neo-input"
              style={{ resize: 'vertical' }}
            />
          </div>

          {sent && (
            <div
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.85rem',
                color: '#34d399',
                border: '1px solid #34d399',
                padding: '8px 12px',
              }}
            >
              Message transmitted to American Hunter Bureau & Kamish Master Node!
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
              SEND MESSAGE
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
