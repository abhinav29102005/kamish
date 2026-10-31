import React from 'react';
import { useKamish } from '../context/useKamish';

export default function Header({ onNavigate, onOpenContact }) {
  const { nodes } = useKamish();
  const activeRegions = Object.values(nodes).filter(node => node.status === 'HEALTHY').length;

  const totalCommitted = Object.values(nodes).reduce(
    (acc, node) => acc + (node.localCommittedOffset || 0),
    0
  );

  return (
    <header
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '1.75rem 3.5rem 0.5rem',
        background: 'transparent',
        position: 'relative',
        zIndex: 50,
        maxWidth: '1600px',
        margin: '0 auto',
        width: '100%',
        gap: '1rem',
        flexWrap: 'wrap',
      }}
    >
      {/* Top Left: Brutalist Logo Box */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '1.5rem' }}>
        <button type="button"
          className="brutal-logo-box"
          onClick={() => onNavigate('home')}
          style={{ cursor: 'pointer' }}
        >
          KAMISH
        </button>

        {/* Live Cluster Health Badge */}
        <div
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: '8px',
            background: 'rgba(15, 23, 42, 0.75)',
            border: '1px solid rgba(168, 85, 247, 0.3)',
            borderRadius: '9999px',
            padding: '5px 14px',
            fontFamily: 'var(--font-mono)',
            fontSize: '0.78rem',
            color: '#e2d9f3',
            letterSpacing: '0.06em',
          }}
        >
          <span
            style={{
              width: '8px',
              height: '8px',
              borderRadius: '50%',
              background: '#22c55e',
              boxShadow: '0 0 10px #22c55e',
              display: 'inline-block',
            }}
          />
          <span>{activeRegions} SIMULATED REGIONS HEALTHY</span>
          <span style={{ color: '#94a3b8' }}>·</span>
          <span style={{ color: '#c084fc', fontWeight: 600 }}>
            {totalCommitted.toLocaleString()} OFFSETS
          </span>
        </div>
      </div>

      {/* Top Right: 3D Story Link & Contact Pill */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '1.25rem' }}>
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
            fontSize: '0.92rem',
            fontWeight: 700,
            letterSpacing: '0.08em',
            padding: '9px 26px',
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
