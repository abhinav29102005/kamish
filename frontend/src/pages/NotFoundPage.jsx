import React from 'react';
import KamishDragon3D from '../components/KamishDragon3D';

export default function NotFoundPage({ onBack }) {
  return (
    <div
      style={{
        maxWidth: '1240px',
        margin: '0 auto',
        padding: '3rem 3.5rem 120px',
        display: 'grid',
        gridTemplateColumns: '1fr 1.2fr',
        alignItems: 'center',
        minHeight: 'calc(100vh - 140px)',
        gap: '2rem',
      }}
    >
      {/* Left: 404 Text matching Page 8 */}
      <div>
        <div
          style={{
            fontFamily: 'var(--font-main)',
            fontSize: 'clamp(5rem, 12vw, 9.5rem)',
            fontWeight: 900,
            lineHeight: 0.9,
            color: '#ffffff',
            letterSpacing: '0.04em',
            marginBottom: '1.5rem',
          }}
        >
          404
        </div>

        <p
          style={{
            fontFamily: 'var(--font-mono)',
            fontSize: '1.25rem',
            color: '#e2d9f3',
            marginBottom: '2.5rem',
          }}
        >
          Route not found.
        </p>

        <button
          onClick={onBack}
          className="brutal-btn-primary"
          style={{
            padding: '1rem 2.5rem',
            fontSize: '0.95rem',
            fontWeight: 800,
          }}
        >
          BACK TO BASE
        </button>
      </div>

      {/* Right: Kamish Dragon matching Page 8 */}
      <div style={{ width: '100%', height: '500px' }}>
        <KamishDragon3D isFlameOn={false} isShadowForm={true} />
      </div>
    </div>
  );
}
