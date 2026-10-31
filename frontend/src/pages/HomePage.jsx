import React, { useState } from 'react';
import KamishDragon3D from '../components/KamishDragon3D';

export default function HomePage({ onNavigate }) {
  const [isFlameOn, setIsFlameOn] = useState(false);
  const [isShadowForm, setIsShadowForm] = useState(true);

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
        paddingBottom: '90px',
        position: 'relative',
        textAlign: 'center',
      }}
    >
      {/* Central 3D Real Dragon matching Mockup */}
      <div style={{ width: '100%', maxWidth: '850px', position: 'relative' }}>
        <KamishDragon3D
          isFlameOn={isFlameOn}
          isShadowForm={isShadowForm}
        />
      </div>

      {/* Subtitle matching Mockup: HUMANITY'S GREATEST CALAMITY */}
      <div
        style={{
          fontFamily: 'var(--font-mono)',
          fontSize: '0.95rem',
          letterSpacing: '0.32em',
          color: '#e2d9f3',
          marginTop: '-1rem',
          marginBottom: '2rem',
          textTransform: 'uppercase',
          fontWeight: 600,
          textShadow: '0 0 12px rgba(192, 132, 252, 0.4)',
        }}
      >
        HUMANITY'S GREATEST CALAMITY
      </div>

      {/* Action Button matching Mockup: FLAME ON → */}
      <div style={{ marginBottom: '2rem' }}>
        <button
          onClick={handleToggleFlame}
          className="brutal-btn-primary"
          style={{
            padding: '1rem 2.8rem',
            fontSize: '1.05rem',
            fontWeight: 900,
            letterSpacing: '0.12em',
          }}
        >
          {isFlameOn ? 'FLAME ON 🔥' : 'FLAME ON →'}
        </button>
      </div>

      {/* Form toggle pill */}
      <div style={{ marginTop: '0.5rem' }}>
        <button
          onClick={() => setIsShadowForm(!isShadowForm)}
          style={{
            background: 'transparent',
            border: 'none',
            color: 'var(--text-dim)',
            fontFamily: 'var(--font-mono)',
            fontSize: '0.78rem',
            cursor: 'pointer',
            textDecoration: 'underline',
          }}
        >
          {isShadowForm ? 'Switch to Apex Crimson Dragon Form' : 'Switch to Shadow Monarch Form'}
        </button>
      </div>
    </div>
  );
}
