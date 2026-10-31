import React, { useState } from 'react';

export default function SignInPage({ onSignedIn }) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [signedIn, setSignedIn] = useState(false);

  const handleSubmit = (e) => {
    e.preventDefault();
    setSignedIn(true);
    setTimeout(() => {
      onSignedIn();
    }, 1000);
  };

  return (
    <div
      style={{
        maxWidth: '680px',
        margin: '0 auto',
        padding: '2.5rem 2rem 120px',
      }}
    >
      {/* Title matching Page 7 */}
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
        SIGN IN
      </h1>

      {/* Form Card matching Page 7 */}
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
              EMAIL
            </label>
            <input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="operator@kamish-broker.net"
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
              PASSWORD
            </label>
            <input
              type="password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••••••••"
              className="neo-input"
            />
          </div>

          {signedIn && (
            <div
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: '0.85rem',
                color: '#34d399',
                border: '1px solid #34d399',
                padding: '8px 12px',
              }}
            >
              Authentication successful! Redirecting to Dashboard...
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
              SIGN IN
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
