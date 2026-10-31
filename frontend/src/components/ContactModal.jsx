import React, { useState } from 'react';
import { X, Send } from 'lucide-react';

export default function ContactModal({ isOpen, onClose, onProduceEvent }) {
  const [topic, setTopic] = useState('telemetry.s-rank');
  const [region, setRegion] = useState('EU_WEST');
  const [policy, setPolicy] = useState('strict-local');
  const [payload, setPayload] = useState('DRAGON_MANA_SIGNATURE: 9.8e7_JOULES');
  const [logStatus, setLogStatus] = useState(null);

  if (!isOpen) return null;

  const handleSubmit = (e) => {
    e.preventDefault();
    const result = {
      id: Math.floor(Math.random() * 900000) + 100000,
      topic,
      origin_region: region,
      residency_rule: policy,
      payload,
      timestamp_ms: Date.now(),
    };
    onProduceEvent(result);
    setLogStatus(`Demo event #${result.id} submitted to the local callback. No broker enforcement is confirmed.`);
  };

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 1000,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'rgba(5, 6, 9, 0.75)',
        backdropFilter: 'blur(12px)',
      }}
      onClick={onClose}
    >
      <div
        className="brutal-card"
        style={{
          width: '90%',
          maxWidth: '650px',
          background: 'rgba(16, 18, 26, 0.95)',
          border: '1px solid rgba(255, 255, 255, 0.2)',
          boxShadow: '0 25px 60px rgba(0, 0, 0, 0.8), 0 0 30px rgba(255, 42, 75, 0.25)',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            borderBottom: '1px solid rgba(255, 255, 255, 0.1)',
            paddingBottom: '1rem',
            marginBottom: '1.25rem',
          }}
        >
          <div>
            <h2
              style={{
                fontFamily: 'var(--font-hero)',
                fontSize: '1.4rem',
                letterSpacing: '0.04em',
                color: '#fff',
              }}
            >
              HUNTER BUREAU CONTROL & INGESTION
            </h2>
            <p style={{ fontFamily: 'var(--font-mono)', fontSize: '0.75rem', color: 'var(--text-muted)' }}>
              SOVEREIGN BROKER DISPATCH · KAMISH S-RANK PROTOCOL
            </p>
          </div>
          <button
            onClick={onClose}
            className="tactile-btn tactile-btn-ghost"
            style={{ padding: '6px 10px', borderRadius: '50%' }}
          >
            <X size={18} />
          </button>
        </div>

        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
          <div>
            <label style={{ display: 'block', fontSize: '0.8rem', fontWeight: 700, marginBottom: '6px' }}>
              EVENT TOPIC
            </label>
            <input
              type="text"
              value={topic}
              onChange={(e) => setTopic(e.target.value)}
              className="neo-well"
              style={{
                width: '100%',
                color: '#fff',
                fontFamily: 'var(--font-mono)',
                fontSize: '0.85rem',
                outline: 'none',
              }}
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
            <div>
              <label style={{ display: 'block', fontSize: '0.8rem', fontWeight: 700, marginBottom: '6px' }}>
                ORIGIN REGION
              </label>
              <select
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                className="neo-well"
                style={{
                  width: '100%',
                  color: '#fff',
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.85rem',
                  outline: 'none',
                  background: '#0a0c12',
                }}
              >
                <option value="EU_WEST">EU_WEST (Frankfurt)</option>
                <option value="US_EAST">US_EAST (N. Virginia)</option>
                <option value="AP_SOUTH">AP_SOUTH (Mumbai)</option>
              </select>
            </div>

            <div>
              <label style={{ display: 'block', fontSize: '0.8rem', fontWeight: 700, marginBottom: '6px' }}>
                RESIDENCY POLICY
              </label>
              <select
                value={policy}
                onChange={(e) => setPolicy(e.target.value)}
                className="neo-well"
                style={{
                  width: '100%',
                  color: '#fff',
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.85rem',
                  outline: 'none',
                  background: '#0a0c12',
                }}
              >
                <option value="strict-local">StrictLocal (Zero Egress)</option>
                <option value="restricted-eu-us">Restricted (EU + US)</option>
                <option value="global-cleartext">GlobalCleartext</option>
                <option value="sealed-ciphertext">SealedCiphertext (Encrypted)</option>
              </select>
            </div>
          </div>

          <div>
            <label style={{ display: 'block', fontSize: '0.8rem', fontWeight: 700, marginBottom: '6px' }}>
              PAYLOAD (MANA / TELEMETRY DATA)
            </label>
            <textarea
              rows={3}
              value={payload}
              onChange={(e) => setPayload(e.target.value)}
              className="neo-well"
              style={{
                width: '100%',
                color: '#fff',
                fontFamily: 'var(--font-mono)',
                fontSize: '0.82rem',
                outline: 'none',
                resize: 'none',
              }}
            />
          </div>

          {logStatus && (
            <div
              className="neo-well"
              style={{
                borderLeft: '4px solid #10b981',
                padding: '8px 12px',
                fontFamily: 'var(--font-mono)',
                fontSize: '0.78rem',
                color: '#34d399',
              }}
            >
              {logStatus}
            </div>
          )}

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: '0.5rem' }}>
            <span style={{ fontSize: '0.72rem', color: 'var(--text-muted)' }}>
              Audit chain verified with SHA-256 zero-payload hash logging.
            </span>
            <button type="submit" className="tactile-btn tactile-btn-crimson">
              <Send size={15} /> Dispatch Stream Event
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
