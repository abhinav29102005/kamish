import React from 'react';

export default function Header({ onNavigate, onOpenSignIn, onOpenContact }) {
  return (
    <header
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '2.5rem 3.5rem 1rem',
        background: 'transparent',
        position: 'relative',
        zIndex: 50,
      }}
    >
      {/* Top Left: Brutalist Logo Box */}
      <div
        className="brutal-logo-box"
        onClick={() => onNavigate('home')}
      >
        KAMISH
      </div>

      {/* Top Right: Sign in & Contact */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '2rem' }}>
        <button
          onClick={onOpenSignIn}
          style={{
            background: 'transparent',
            border: 'none',
            color: '#ffffff',
            fontFamily: 'var(--font-main)',
            fontSize: '1.05rem',
            fontWeight: 500,
            cursor: 'pointer',
            padding: '4px 8px',
            transition: 'color 0.2s ease',
          }}
          onMouseEnter={(e) => (e.target.style.color = '#c084fc')}
          onMouseLeave={(e) => (e.target.style.color = '#ffffff')}
        >
          Sign in
        </button>

        <button
          onClick={onOpenContact}
          style={{
            background: 'rgba(25, 14, 40, 0.75)',
            backdropFilter: 'blur(10px)',
            border: '1px solid rgba(255, 255, 255, 0.15)',
            boxShadow: '0 4px 15px rgba(0, 0, 0, 0.4), inset 0 1px 0 rgba(255, 255, 255, 0.1)',
            color: '#ffffff',
            fontFamily: 'var(--font-main)',
            fontSize: '1.05rem',
            fontWeight: 600,
            padding: '8px 24px',
            borderRadius: '9999px',
            cursor: 'pointer',
            transition: 'all 0.2s ease',
          }}
          onMouseEnter={(e) => {
            e.target.style.borderColor = '#c084fc';
            e.target.style.boxShadow = '0 6px 20px rgba(168, 85, 247, 0.35)';
          }}
          onMouseLeave={(e) => {
            e.target.style.borderColor = 'rgba(255, 255, 255, 0.15)';
            e.target.style.boxShadow = '0 4px 15px rgba(0, 0, 0, 0.4)';
          }}
        >
          Contact
        </button>
      </div>
    </header>
  );
}
