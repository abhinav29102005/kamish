import React, { useState } from 'react';
import KamishDragon3D from '../components/KamishDragon3D';

export default function HomePage({ onNavigate }) {
  const [isKillMode, setIsKillMode] = useState(false);

  const handleToggleKillMode = () => {
    setIsKillMode((prev) => !prev);
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
        transition: 'background 0.5s ease',
        background: isKillMode
          ? 'radial-gradient(ellipse at 50% 40%, rgba(153, 27, 27, 0.22) 0%, rgba(9, 3, 10, 0.98) 75%)'
          : 'transparent',
      }}
    >
      {/* Massive 3D Dragon Container */}
      <div
        style={{
          width: '100%',
          maxWidth: '1240px',
          position: 'relative',
          marginBottom: '0.5rem',
        }}
      >
        <KamishDragon3D
          isFlameOn={isKillMode}
          isKillMode={isKillMode}
        />
      </div>

      {/* Subtitle / Threat Alert matching Mockup */}
      <div
        style={{
          fontFamily: 'var(--font-mono)',
          fontSize: '1.05rem',
          letterSpacing: isKillMode ? '0.36em' : '0.32em',
          color: isKillMode ? '#ff4d6d' : '#e2d9f3',
          marginBottom: '2rem',
          textTransform: 'uppercase',
          fontWeight: 700,
          textShadow: isKillMode
            ? '0 0 18px rgba(255, 30, 56, 0.8), 0 0 35px rgba(255, 30, 56, 0.4)'
            : '0 0 14px rgba(192, 132, 252, 0.45)',
          transition: 'all 0.35s ease',
        }}
      >
        {isKillMode ? '⚠ S-RANK CALAMITY: KILL MODE ENGAGED ⚠' : "HUMANITY'S GREATEST CALAMITY"}
      </div>

      {/* Action Button: KILL MODE */}
      <div style={{ marginBottom: '1.5rem' }}>
        <button
          onClick={handleToggleKillMode}
          style={{
            fontFamily: 'var(--font-mono)',
            fontSize: '1.1rem',
            fontWeight: 900,
            letterSpacing: '0.14em',
            textTransform: 'uppercase',
            padding: '1.15rem 3.4rem',
            background: isKillMode ? '#ff1e38' : '#c084fc',
            color: isKillMode ? '#ffffff' : '#0a0513',
            border: '2.5px solid #ffffff',
            boxShadow: isKillMode
              ? '6px 6px 0px #7f1d1d, 0 0 25px rgba(255, 30, 56, 0.6)'
              : '6px 6px 0px #4c1d95',
            cursor: 'pointer',
            transition: 'all 0.2s cubic-bezier(0.16, 1, 0.3, 1)',
            outline: 'none',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.transform = 'translate(-2px, -2px)';
            e.currentTarget.style.boxShadow = isKillMode
              ? '8px 8px 0px #7f1d1d, 0 0 35px rgba(255, 30, 56, 0.8)'
              : '8px 8px 0px #4c1d95, 0 0 20px rgba(168, 85, 247, 0.4)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.transform = 'translate(0px, 0px)';
            e.currentTarget.style.boxShadow = isKillMode
              ? '6px 6px 0px #7f1d1d, 0 0 25px rgba(255, 30, 56, 0.6)'
              : '6px 6px 0px #4c1d95';
          }}
          onMouseDown={(e) => {
            e.currentTarget.style.transform = 'translate(4px, 4px)';
            e.currentTarget.style.boxShadow = isKillMode
              ? '2px 2px 0px #7f1d1d'
              : '2px 2px 0px #4c1d95';
          }}
          onMouseUp={(e) => {
            e.currentTarget.style.transform = 'translate(-2px, -2px)';
          }}
        >
          {isKillMode ? 'DISENGAGE KILL MODE ⚡' : 'KILL MODE 🔥'}
        </button>
      </div>

      {/* Mode Status Pill */}
      <div
        style={{
          fontFamily: 'var(--font-mono)',
          fontSize: '0.8rem',
          letterSpacing: '0.15em',
          color: isKillMode ? '#ff8599' : 'var(--text-dim)',
          textTransform: 'uppercase',
        }}
      >
        {isKillMode ? 'ALL TARGETS LOCKED · APOCALYPTIC INFERNO' : 'DRAG TO ORBIT · REAL-TIME 3D MODEL'}
      </div>
    </div>
  );
}
