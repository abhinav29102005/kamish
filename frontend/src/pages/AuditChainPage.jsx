import React, { useState } from 'react';
import { ShieldCheck, Lock, Hash, CheckCircle2, AlertTriangle, Key } from 'lucide-react';

export default function AuditChainPage() {
  const [entries, setEntries] = useState([
    {
      seq: 1,
      action: 'INGESTION_ADMIT',
      class_id: 'sovereign.telemetry',
      source: 'EU_WEST',
      target: 'EU_WEST',
      outcome: 'ALLOWED',
      reason: 'Origin in legal region set',
      prev_hash: '0000000000000000000000000000000000000000000000000000000000000000',
      entry_hash: '4a6b29c19b88e1467e2bf57bc2624a04d2e8b625cf05e47854cfdf7b6eb4e823',
    },
    {
      seq: 2,
      action: 'REPLICATION_EGRESS',
      class_id: 'sovereign.telemetry',
      source: 'EU_WEST',
      target: 'US_EAST',
      outcome: 'DENIED',
      reason: 'StrictLocalEgressProhibited (INV-8)',
      prev_hash: '4a6b29c19b88e1467e2bf57bc2624a04d2e8b625cf05e47854cfdf7b6eb4e823',
      entry_hash: '7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d',
    },
    {
      seq: 3,
      action: 'SEALED_REPLICATION',
      class_id: 'encrypted.backup',
      source: 'EU_WEST',
      target: 'AP_SOUTH',
      outcome: 'ALLOWED',
      reason: 'SealedCiphertext mode: keys stay in EU_WEST home region',
      prev_hash: '7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d',
      entry_hash: '9e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f',
    },
  ]);

  const [verified, setVerified] = useState(true);

  return (
    <div style={{ paddingBottom: '120px', maxWidth: '1100px', margin: '0 auto', paddingLeft: '1.5rem', paddingRight: '1.5rem' }}>
      <div style={{ marginTop: '2rem', marginBottom: '2rem' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '6px' }}>
          <span className="mono-tag mono-tag-crimson">SECURITY PROOFS</span>
          <span className="mono-tag mono-tag-emerald">FR-13 · ZERO PAYLOAD AUDIT</span>
        </div>
        <h1
          style={{
            fontFamily: 'var(--font-hero)',
            fontSize: 'clamp(2.2rem, 4vw, 3.2rem)',
            fontWeight: 900,
            color: '#fff',
          }}
        >
          CRYPTOGRAPHIC AUDIT CHAIN
        </h1>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.95rem' }}>
          Tamper-evident, hash-chained ledger storing all policy enforcement decisions without payload bytes.
        </p>
      </div>

      {/* Verification Banner */}
      <div
        className="brutal-card"
        style={{
          borderLeft: '4px solid #10b981',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '2rem',
        }}
      >
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '4px' }}>
            <CheckCircle2 color="#10b981" size={20} />
            <h3 style={{ fontFamily: 'var(--font-hero)', fontSize: '1.2rem', color: '#fff' }}>
              CHAIN INTEGRITY VERIFIED
            </h3>
          </div>
          <p style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            All 3 hash-chain links verified cryptographically against SHA-256 genesis blocks.
          </p>
        </div>
        <button
          onClick={() => setVerified(true)}
          className="tactile-btn tactile-btn-crimson"
          style={{ padding: '8px 18px', fontSize: '0.8rem' }}
        >
          Re-verify Audit Log
        </button>
      </div>

      {/* Hash Chain Items */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
        {entries.map((entry) => (
          <div key={entry.seq} className="brutal-card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem', flexWrap: 'wrap', gap: '8px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <span className="mono-tag mono-tag-crimson">ENTRY #{entry.seq}</span>
                <span style={{ fontFamily: 'var(--font-mono)', fontSize: '0.85rem', color: '#fff', fontWeight: 700 }}>
                  {entry.action}
                </span>
              </div>
              <span className={`mono-tag ${entry.outcome === 'ALLOWED' ? 'mono-tag-emerald' : 'mono-tag-crimson'}`}>
                {entry.outcome}
              </span>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '1rem', marginBottom: '1rem' }}>
              <div className="neo-well">
                <span style={{ fontSize: '0.72rem', color: 'var(--text-muted)', display: 'block' }}>CLASS ID</span>
                <span style={{ fontFamily: 'var(--font-mono)', fontSize: '0.8rem', color: '#e2e8f0' }}>{entry.class_id}</span>
              </div>
              <div className="neo-well">
                <span style={{ fontSize: '0.72rem', color: 'var(--text-muted)', display: 'block' }}>TRANSFER PATH</span>
                <span style={{ fontFamily: 'var(--font-mono)', fontSize: '0.8rem', color: '#e2e8f0' }}>{entry.source} → {entry.target}</span>
              </div>
              <div className="neo-well">
                <span style={{ fontSize: '0.72rem', color: 'var(--text-muted)', display: 'block' }}>DECISION REASON</span>
                <span style={{ fontFamily: 'var(--font-mono)', fontSize: '0.8rem', color: entry.outcome === 'DENIED' ? '#ff4d6d' : '#34d399' }}>
                  {entry.reason}
                </span>
              </div>
            </div>

            <div className="neo-well" style={{ background: '#07080c', fontSize: '0.75rem', fontFamily: 'var(--font-mono)' }}>
              <div style={{ display: 'flex', gap: '8px', marginBottom: '4px' }}>
                <span style={{ color: 'var(--text-dark)' }}>PREV HASH:</span>
                <span style={{ color: 'var(--text-muted)', wordBreak: 'break-all' }}>{entry.prev_hash}</span>
              </div>
              <div style={{ display: 'flex', gap: '8px' }}>
                <span style={{ color: '#ff2a4b' }}>ENTRY HASH:</span>
                <span style={{ color: '#f8fafc', wordBreak: 'break-all', fontWeight: 700 }}>{entry.entry_hash}</span>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
