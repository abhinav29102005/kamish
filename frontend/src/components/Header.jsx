import React from 'react';

export default function Header({ onNavigate, onOpenContact }) {
  return (
    <header
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '2rem 3.5rem 0.5rem',
        background: 'transparent',
        position: 'relative',
        zIndex: 50,
        maxWidth: '1600px',
        margin: '0 auto',
        width: '100%',
      }}
    >
      {/* Top Left: Brutalist Logo Box */}
      <div
        className="brutal-logo-box"
        onClick={() => onNavigate('home')}
        style={{ cursor: 'pointer' }}
      >
        KAMISH
      </div>

      {/* Top Right: 3D Story Link & Contact Pill (Sign in removed) */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '1.5rem' }}>
        <a
          href="/3d.html"
          target="_blank"
          rel="noopener noreferrer"
          style={{
            background: 'rgba(255, 107, 53, 0.15)',
            border: '1px solid rgba(255, 107, 53, 0.5)',
            color: '#ff6b35',
            fontFamily: 'var(--font-mono, monospace)',
            fontSize: '0.85rem',
            fontWeight: 700,
            letterSpacing: '0.05em',
            padding: '8px 18px',
            borderRadius: '9999px',
            textDecoration: 'none',
            display: 'inline-flex',
            alignItems: 'center',
            gap: '6px',
            transition: 'all 0.2s ease',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = '#ff6b35';
            e.currentTarget.style.color = '#000000';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'rgba(255, 107, 53, 0.15)';
            e.currentTarget.style.color = '#ff6b35';
          }}
        >
          3D Story ↗
        </a>

        <button
          onClick={onOpenContact}
          style={{
            background: 'rgba(25, 14, 40, 0.75)',
            backdropFilter: 'blur(12px)',
            border: '1.5px solid rgba(192, 132, 252, 0.35)',
            boxShadow: '0 4px 18px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.15)',
            color: '#ffffff',
            fontFamily: 'var(--font-mono)',
            fontSize: '0.95rem',
            fontWeight: 700,
            letterSpacing: '0.08em',
            padding: '10px 28px',
            borderRadius: '9999px',
            cursor: 'pointer',
            transition: 'all 0.25s ease',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.borderColor = '#c084fc';
            e.currentTarget.style.transform = 'translateY(-2px)';
            e.currentTarget.style.boxShadow = '0 6px 24px rgba(168, 85, 247, 0.5)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.borderColor = 'rgba(192, 132, 252, 0.35)';
            e.currentTarget.style.transform = 'translateY(0)';
            e.currentTarget.style.boxShadow = '0 4px 18px rgba(0, 0, 0, 0.5)';
          }}
        >
          Contact
        </button>
      </div>
    </header>
  );
}
