import React, { useState } from 'react';
import { useKamish } from '../context/useKamish';
import type { DecisionOutcome } from '../types';
import { ShieldCheck, Search, Filter, Lock, CheckCircle2, AlertTriangle, XCircle, Hash } from 'lucide-react';

export const SovereigntyAuditLedger: React.FC = () => {
  const { auditLedger, verifyAuditLedgerIntegrity } = useKamish();
  const [filterOutcome, setFilterOutcome] = useState<string>('ALL');
  const [searchTerm, setSearchTerm] = useState<string>('');
  const [verificationResult, setVerificationResult] = useState<{
    valid?: boolean;
    count?: number;
    error?: string;
  } | null>(null);
  const [isVerifying, setIsVerifying] = useState<boolean>(false);

  const filtered = auditLedger.filter((entry) => {
    if (filterOutcome !== 'ALL' && entry.outcome !== filterOutcome) return false;
    if (searchTerm) {
      const term = searchTerm.toLowerCase();
      return (
        entry.topic.toLowerCase().includes(term) ||
        entry.action.toLowerCase().includes(term) ||
        entry.sourceRegion.toLowerCase().includes(term) ||
        entry.targetRegion.toLowerCase().includes(term) ||
        entry.reason.toLowerCase().includes(term) ||
        entry.hash.toLowerCase().includes(term)
      );
    }
    return true;
  });

  const handleVerify = async () => {
    setIsVerifying(true);
    setVerificationResult(null);
    const result = await verifyAuditLedgerIntegrity();
    setIsVerifying(false);
    setVerificationResult(result);
  };

  const getOutcomeBadge = (outcome: DecisionOutcome) => {
    switch (outcome) {
      case 'AllowCleartext':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
            <CheckCircle2 className="w-3 h-3" />
            ALLOW_CLEARTEXT
          </span>
        );
      case 'AllowSealed':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-medium bg-purple-500/10 text-purple-300 border border-purple-500/20">
            <Lock className="w-3 h-3" />
            ALLOW_SEALED
          </span>
        );
      case 'Deny':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-medium bg-rose-500/10 text-rose-400 border border-rose-500/20">
            <XCircle className="w-3 h-3" />
            DENY (BLOCKED)
          </span>
        );
    }
  };

  return (
    <div className="bg-[#080512] border border-white/5 rounded-xl p-3.5 space-y-3.5">
      {/* Header & Verification Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/5 pb-3">
        <div className="flex items-center gap-2">
          <ShieldCheck className="w-4 h-4 text-purple-400" />
          <div>
            <h3 className="text-xs font-semibold font-mono uppercase text-slate-100 tracking-wide">
              SHA-256 Decision Audit Ledger
            </h3>
            <span className="text-[10px] text-slate-500 font-mono">
              Enforcement Points E1-E10 · Zero-Payload Security Threat Model (INV-8, INV-9)
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2.5">
          {verificationResult && (
            <div
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-mono font-medium ${
                verificationResult.valid
                  ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
                  : 'bg-rose-500/10 text-rose-300 border border-rose-500/20'
              }`}
            >
              {verificationResult.valid ? (
                <>
                  <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                  <span>Verified: {verificationResult.count} unbroken blocks</span>
                </>
              ) : (
                <>
                  <AlertTriangle className="w-3 h-3 text-rose-400" />
                  <span>{verificationResult.error}</span>
                </>
              )}
            </div>
          )}

          <button
            onClick={handleVerify}
            disabled={isVerifying}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 text-slate-200 border border-white/10 text-xs font-mono font-medium transition-colors"
          >
            <Hash className={`w-3.5 h-3.5 ${isVerifying ? 'animate-spin' : ''}`} />
            <span>{isVerifying ? 'Verifying...' : 'Verify Chain Integrity'}</span>
          </button>
        </div>
      </div>

      {/* Filter and Search Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 text-xs font-mono">
        <div className="flex items-center gap-2 bg-white/[0.03] px-2.5 py-1.5 rounded-lg border border-white/5 flex-1 max-w-md">
          <Search className="w-3.5 h-3.5 text-slate-500" />
          <input
            type="text"
            placeholder="Search topic, action, reason, hash..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="bg-transparent text-slate-200 placeholder-slate-600 outline-none w-full text-xs font-mono"
          />
        </div>

        <div className="flex items-center gap-2">
          <span className="text-slate-500 flex items-center gap-1 text-[11px]">
            <Filter className="w-3 h-3" /> Filter:
          </span>
          {['ALL', 'AllowCleartext', 'AllowSealed', 'Deny'].map((out) => (
            <button
              key={out}
              onClick={() => setFilterOutcome(out)}
              className={`px-2 py-0.5 rounded text-[11px] transition-colors ${
                filterOutcome === out
                  ? 'bg-white/10 text-white border border-white/15 font-medium'
                  : 'text-slate-500 hover:text-slate-300 bg-white/[0.02] border border-white/5'
              }`}
            >
              {out}
            </button>
          ))}
        </div>
      </div>

      {/* Ledger Table */}
      <div className="overflow-x-auto border border-white/5 rounded-lg">
        <table className="w-full text-left text-xs font-mono">
          <thead className="bg-white/[0.02] text-slate-400 uppercase text-[10px] tracking-wider border-b border-white/5">
            <tr>
              <th className="py-2 px-3">Seq</th>
              <th className="py-2 px-3">Action</th>
              <th className="py-2 px-3">Topic</th>
              <th className="py-2 px-3">Source → Target</th>
              <th className="py-2 px-3">Outcome</th>
              <th className="py-2 px-3">Security Reason</th>
              <th className="py-2 px-3">SHA-256 Hash</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-white/5 text-slate-300">
            {filtered.length === 0 ? (
              <tr>
                <td colSpan={7} className="py-8 text-center text-slate-500">
                  No audit entries found matching filter.
                </td>
              </tr>
            ) : (
              filtered.map((entry) => (
                <tr
                  key={entry.seq}
                  className={`hover:bg-white/[0.02] transition-colors ${
                    entry.outcome === 'Deny' ? 'bg-rose-950/10' : ''
                  }`}
                >
                  <td className="py-2 px-3 font-medium text-purple-400">#{entry.seq}</td>
                  <td className="py-2 px-3">
                    <span className="text-slate-200 font-medium">{entry.action}</span>
                  </td>
                  <td className="py-2 px-3 text-slate-300">{entry.topic}</td>
                  <td className="py-2 px-3 text-[11px]">
                    <span className="text-emerald-400">{entry.sourceRegion}</span>
                    <span className="text-slate-500 mx-1">→</span>
                    <span className="text-cyan-400">{entry.targetRegion}</span>
                  </td>
                  <td className="py-2 px-3">{getOutcomeBadge(entry.outcome)}</td>
                  <td className="py-2 px-3 text-slate-400 max-w-xs truncate" title={entry.reason}>
                    {entry.reason}
                  </td>
                  <td className="py-2 px-3 font-mono text-[10px] text-slate-500" title={`Prev: ${entry.prevHash}`}>
                    <span className="text-slate-400">{entry.hash.substring(0, 10)}</span>
                    <span className="text-slate-600">...</span>
                    <span className="text-slate-500">{entry.hash.substring(54)}</span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
