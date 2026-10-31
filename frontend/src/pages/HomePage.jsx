import React, { useState } from 'react';
import KamishDragon3D from '../components/KamishDragon3D';

export default function HomePage({ onNavigate }) {
  const [isFlameOn, setIsFlameOn] = useState(false);

  const handleToggleFlame = () => {
    setIsFlameOn((prev) => !prev);
  };

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        minHeight: 'calc(100vh - 120px)',
        paddingBottom: '110px',
        position: 'relative',
        textAlign: 'center',
      }}
    >
      {/* Massive 3D Flying Dragon Container (Non-movable, Blender animated) */}
      <div
        style={{
          width: '100%',
          maxWidth: '1240px',
          position: 'relative',
          marginBottom: '0.5rem',
        }}
      >
        <KamishDragon3D isFlameOn={isFlameOn} />
      </div>

      {/* Subtitle matching Mockup Page 1: HUMANITY'S GREATEST CALAMITY */}
      <div
        style={{
          fontFamily: 'var(--font-mono)',
          fontSize: '1.05rem',
          letterSpacing: '0.34em',
          color: '#e2d9f3',
          marginBottom: '2rem',
          textTransform: 'uppercase',
          fontWeight: 700,
          textShadow: '0 0 16px rgba(192, 132, 252, 0.45)',
        }}
      >
        HUMANITY'S GREATEST CALAMITY
      </div>

      {/* Action Button matching Mockup Page 1: FLAME ON → */}
      <div style={{ marginBottom: '1.5rem' }}>
        <button
          onClick={handleToggleFlame}
          style={{
            fontFamily: 'var(--font-mono)',
            fontSize: '1.1rem',
            fontWeight: 900,
            letterSpacing: '0.14em',
            textTransform: 'uppercase',
            padding: '1.15rem 3.4rem',
            background: '#c084fc',
            color: '#0a0513',
            border: '2.5px solid #ffffff',
            boxShadow: '6px 6px 0px #4c1d95',
            cursor: 'pointer',
            transition: 'all 0.2s cubic-bezier(0.16, 1, 0.3, 1)',
            outline: 'none',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.transform = 'translate(-2px, -2px)';
            e.currentTarget.style.boxShadow = '8px 8px 0px #4c1d95, 0 0 24px rgba(168, 85, 247, 0.45)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.transform = 'translate(0px, 0px)';
            e.currentTarget.style.boxShadow = '6px 6px 0px #4c1d95';
          }}
          onMouseDown={(e) => {
            e.currentTarget.style.transform = 'translate(4px, 4px)';
            e.currentTarget.style.boxShadow = '2px 2px 0px #4c1d95';
          }}
          onMouseUp={(e) => {
            e.currentTarget.style.transform = 'translate(-2px, -2px)';
          }}
        >
          {isFlameOn ? 'FLAME ON 🔥' : 'FLAME ON →'}
        </button>
      </div>

      {/* Sub-tag */}
      <div
        style={{
          fontFamily: 'var(--font-mono)',
          fontSize: '0.8rem',
          letterSpacing: '0.16em',
          color: 'var(--text-dim)',
          textTransform: 'uppercase',
        }}
      >
        SOVEREIGN EVENT BROKER · S-RANK DRAGON OF SOLO LEVELING
      </div>
    </div>
  );
}
