import React, { useState } from 'react';

export default function ContactPage() {
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [message, setMessage] = useState('');
  const [sent, setSent] = useState(false);

  const handleSubmit = (e) => {
    e.preventDefault();
    setSent(true);
  };

  return (
    <div
      style={{
        maxWidth: '720px',
        margin: '0 auto',
        padding: '2rem 2rem 120px',
      }}
    >
      {/* Title matching Page 6 with Logo */}
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
          CONTACT
        </h1>
      </div>

      {/* Form Card matching Page 6 */}
      <div className="brutal-card-offset" style={{ padding: '2.5rem' }}>
        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '1.75rem' }}>
          <div>
            <label htmlFor="contact-name"
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
            <input id="contact-name"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Thomas Andre / Sung Jin-Woo"
              className="neo-input"
            />
          </div>

          <div>
            <label htmlFor="contact-email"
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
            <input id="contact-email"
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="hunter@national-bureau.gov"
              className="neo-input"
            />
          </div>

          <div>
            <label htmlFor="contact-message"
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
            <textarea id="contact-message"
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
              Local message preview only. Nothing was transmitted or sent.
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
              PREVIEW MESSAGE LOCALLY
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
