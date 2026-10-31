import { useState } from 'react';
import { useKamish } from '../context/useKamish';

export default function AuditChainPage() {
  const { auditLedger, verifyAuditLedgerIntegrity } = useKamish();
  const [result, setResult] = useState(null);
  const verify = async () => {
    try { setResult(await verifyAuditLedgerIntegrity()); }
    catch (error) { setResult({ valid: false, error: error instanceof Error ? error.message : 'Check failed' }); }
  };
  return <section style={{ padding: '2rem 2rem 120px', maxWidth: '1100px', margin: '0 auto' }}>
    <h1>LOCAL DEMO AUDIT</h1>
    <p>Browser-only hash-chain examples. These are not broker audit receipts or hardware/security proofs.</p>
    <button className="brutal-btn-primary" onClick={verify}>CHECK LOCAL DEMO AUDIT</button>
    {result && <p role="status">{result.valid ? `${result.count} local entries checked.` : result.error}</p>}
    {auditLedger.map(entry => <div className="brutal-card" key={entry.seq}>
      <p>#{entry.seq} · {entry.action} · {entry.topic} · {entry.outcome}</p>
      <p>{entry.sourceRegion} → {entry.targetRegion}: {entry.reason}</p>
      <code style={{ overflowWrap: 'anywhere' }}>{entry.hash}</code>
    </div>)}
  </section>;
}
