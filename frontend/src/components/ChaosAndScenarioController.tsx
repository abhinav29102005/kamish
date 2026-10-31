import React, { useState } from 'react';
import { useKamish } from '../context/useKamish';
import type { ResidencyPolicy } from '../types';
import {
  Play,
  RotateCcw,
  Sliders,
  Terminal,
  WifiOff,
  Clock,
  Send,
  Shield,
  Zap,
} from 'lucide-react';

export const ChaosAndScenarioController: React.FC = () => {
  const {
    scenarioState,
    runScenario,
    resetCluster,
    wanDelayMs,
    setWanDelayMs,
    isPartitioned,
    setIsPartitioned,
    adaptiveRoutingActive,
    setAdaptiveRoutingActive,
    trafficRateEps,
    setTrafficRateEps,
    produceCustomEvent,
  } = useKamish();

  // Custom Event Injection State
  const [topic, setTopic] = useState<string>('eu-medical-telemetry');
  const [key, setKey] = useState<string>('patient-berlin-892');
  const [policyType, setPolicyType] = useState<
    'StrictLocal' | 'RestrictedReplication' | 'GlobalCleartext' | 'SealedCiphertext'
  >('StrictLocal');
  const [targetRegion, setTargetRegion] = useState<'US_EAST' | 'AP_SOUTH'>('US_EAST');
  const [injectionFeedback, setInjectionFeedback] = useState<string | null>(null);

  const handleInjectEvent = async (e: React.FormEvent) => {
    e.preventDefault();
    setInjectionFeedback(null);

    const policy: ResidencyPolicy = {
      type: policyType,
      allowedRegions:
        policyType === 'RestrictedReplication' || policyType === 'SealedCiphertext'
          ? [targetRegion]
          : undefined,
      description: `User-defined ${policyType} policy rule`,
    };

    const success = await produceCustomEvent(topic, key, policy, targetRegion);
    if (success) {
      setInjectionFeedback(`✓ Event admitted and dispatched to ${targetRegion} (${policyType})`);
    } else {
      setInjectionFeedback(`✗ Policy Violation: ${policyType} strictly forbids egress to ${targetRegion}`);
    }

    setTimeout(() => setInjectionFeedback(null), 4500);
  };

  return (
    <div className="space-y-3.5">
      {/* 1. Hackathon Scenario Launchpad */}
      <div className="bg-[#080512] border border-white/5 rounded-xl p-3.5">
        <div className="flex flex-wrap items-center justify-between gap-3 mb-3">
          <div>
            <h3 className="text-xs font-semibold font-mono uppercase text-slate-100 flex items-center gap-2">
              <Zap className="w-3.5 h-3.5 text-purple-400" />
              1-Click Demonstration Scenarios (Gates 2–4)
            </h3>
            <span className="text-[10px] text-slate-500 font-mono">
              Verifies zero-loss catch-up, dynamic lag simulation, and sovereign residency enforcement
            </span>
          </div>

          <button
            onClick={resetCluster}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-white/5 hover:bg-white/10 text-slate-300 border border-white/10 text-xs font-mono transition-colors"
          >
            <RotateCcw className="w-3 h-3" />
            Reset Cluster
          </button>
        </div>

        {/* 3 Scenario Cards */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-2.5">
          {/* Scenario A */}
          <div
            className={`p-3 rounded-lg border transition-colors flex flex-col justify-between ${
              scenarioState.id === 'A'
                ? 'bg-purple-950/20 border-purple-500/40'
                : 'bg-white/[0.02] border-white/5 hover:border-white/10'
            }`}
          >
            <div>
              <div className="flex items-center justify-between mb-1">
                <span className="text-[10px] font-mono font-medium px-1.5 py-0.2 rounded bg-purple-500/15 text-purple-300 border border-purple-500/20">
                  GATE 2 / DEMO A
                </span>
                {scenarioState.id === 'A' && (
                  <span className="text-[10px] font-mono font-medium text-purple-300">
                    {scenarioState.status.toUpperCase()}
                  </span>
                )}
              </div>
              <h4 className="font-medium text-xs text-slate-100 mb-1">Residency Enforcement</h4>
              <p className="text-[11px] text-slate-400 leading-relaxed mb-3">
                Ingests <code className="text-emerald-400 font-mono">StrictLocal</code> sovereign record in EU_WEST.
                Replication egress to US_EAST is blocked with tamper-evident audit trail.
              </p>
            </div>
            <button
              onClick={() => runScenario('A')}
              disabled={scenarioState.status === 'running'}
              className="w-full flex items-center justify-center gap-1.5 py-1.5 rounded-md bg-white/10 hover:bg-white/15 text-white font-medium font-mono text-xs transition-colors disabled:opacity-40"
            >
              <Play className="w-3 h-3 fill-current" />
              Run Demo A
            </button>
          </div>

          {/* Scenario B */}
          <div
            className={`p-3 rounded-lg border transition-colors flex flex-col justify-between ${
              scenarioState.id === 'B'
                ? 'bg-amber-950/20 border-amber-500/40'
                : 'bg-white/[0.02] border-white/5 hover:border-white/10'
            }`}
          >
            <div>
              <div className="flex items-center justify-between mb-1">
                <span className="text-[10px] font-mono font-medium px-1.5 py-0.2 rounded bg-amber-500/15 text-amber-300 border border-amber-500/20">
                  GATE 3 / DEMO B
                </span>
                {scenarioState.id === 'B' && (
                  <span className="text-[10px] font-mono font-medium text-amber-300">
                    {scenarioState.status.toUpperCase()}
                  </span>
                )}
              </div>
              <h4 className="font-medium text-xs text-slate-100 mb-1">Slow-Link & Dynamic Lag</h4>
              <p className="text-[11px] text-slate-400 leading-relaxed mb-3">
                Injects 300ms WAN delay via chaos proxy. Local produce stays unblocked (&lt;1ms) while
                replication lag rises to 50 on the meter.
              </p>
            </div>
            <button
              onClick={() => runScenario('B')}
              disabled={scenarioState.status === 'running'}
              className="w-full flex items-center justify-center gap-1.5 py-1.5 rounded-md bg-white/10 hover:bg-white/15 text-white font-medium font-mono text-xs transition-colors disabled:opacity-40"
            >
              <Play className="w-3 h-3 fill-current" />
              Run Demo B
            </button>
          </div>

          {/* Scenario C */}
          <div
            className={`p-3 rounded-lg border transition-colors flex flex-col justify-between ${
              scenarioState.id === 'C'
                ? 'bg-emerald-950/20 border-emerald-500/40'
                : 'bg-white/[0.02] border-white/5 hover:border-white/10'
            }`}
          >
            <div>
              <div className="flex items-center justify-between mb-1">
                <span className="text-[10px] font-mono font-medium px-1.5 py-0.2 rounded bg-emerald-500/15 text-emerald-300 border border-emerald-500/20">
                  GATE 4 / DEMO C
                </span>
                {scenarioState.id === 'C' && (
                  <span className="text-[10px] font-mono font-medium text-emerald-300">
                    {scenarioState.status.toUpperCase()}
                  </span>
                )}
              </div>
              <h4 className="font-medium text-xs text-slate-100 mb-1">Outage & Zero-Loss Recovery</h4>
              <p className="text-[11px] text-slate-400 leading-relaxed mb-3">
                Severs remote US_EAST broker, buffers 15 records in durable outbox, heals connection, and catches up
                from watermark with 0 data loss.
              </p>
            </div>
            <button
              onClick={() => runScenario('C')}
              disabled={scenarioState.status === 'running'}
              className="w-full flex items-center justify-center gap-1.5 py-1.5 rounded-md bg-white/10 hover:bg-white/15 text-white font-medium font-mono text-xs transition-colors disabled:opacity-40"
            >
              <Play className="w-3 h-3 fill-current" />
              Run Demo C
            </button>
          </div>
        </div>

        {/* Live Scenario Terminal Log Output */}
        {scenarioState.logs.length > 0 && (
          <div className="mt-3 bg-black/60 rounded-lg border border-white/5 p-3 font-mono text-xs">
            <div className="flex items-center justify-between pb-2 mb-2 border-b border-white/5 text-slate-400 text-[11px]">
              <div className="flex items-center gap-2">
                <Terminal className="w-3 h-3 text-purple-400" />
                <span>Kamish Terminal — {scenarioState.name}</span>
              </div>
              <span className="text-emerald-400 font-medium">{scenarioState.gate}</span>
            </div>
            <div className="max-h-40 overflow-y-auto space-y-1 text-slate-300">
              {scenarioState.logs.map((log, idx) => (
                <div
                  key={idx}
                  className={
                    log.includes('PASSED')
                      ? 'text-emerald-400 font-medium'
                      : log.includes('BLOCKED')
                      ? 'text-rose-400 font-medium'
                      : log.includes('DEGRADED') || log.includes('300ms')
                      ? 'text-amber-400'
                      : 'text-slate-400'
                  }
                >
                  {log}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* 2. Chaos Injection Lab & Custom Event Injector Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
        {/* Chaos Controller Panel */}
        <div className="bg-[#080512] border border-white/5 rounded-xl p-3.5 space-y-3.5">
          <div className="flex items-center gap-2 border-b border-white/5 pb-2">
            <Sliders className="w-3.5 h-3.5 text-rose-400" />
            <h3 className="text-xs font-semibold font-mono uppercase text-slate-100">
              Synthetic Chaos & WAN Tuning Lab
            </h3>
          </div>

          <div className="space-y-3.5 text-xs font-mono">
            {/* WAN Latency Slider */}
            <div>
              <div className="flex items-center justify-between mb-1.5">
                <span className="text-slate-400 flex items-center gap-1.5 text-[11px]">
                  <Clock className="w-3 h-3 text-purple-400" /> Injected WAN Delay (US_EAST link):
                </span>
                <span className="font-semibold text-slate-200 bg-white/5 px-2 py-0.5 rounded border border-white/5">
                  {wanDelayMs} ms
                </span>
              </div>
              <input
                type="range"
                min="0"
                max="600"
                step="25"
                value={wanDelayMs}
                onChange={(e) => setWanDelayMs(Number(e.target.value))}
                className="w-full accent-purple-400 h-1 bg-white/10 rounded-lg cursor-pointer"
              />
              <div className="flex justify-between text-[10px] text-slate-500 mt-1">
                <span>0ms (Healthy Fiber)</span>
                <span>300ms (High Delay)</span>
                <span>600ms (Satellite)</span>
              </div>
            </div>

            {/* Network Partition Severance Toggle */}
            <div className="flex items-center justify-between p-2 rounded-lg bg-white/[0.02] border border-white/5">
              <div className="flex items-center gap-2">
                <WifiOff className={`w-3.5 h-3.5 ${isPartitioned ? 'text-rose-400' : 'text-slate-500'}`} />
                <div>
                  <span className="text-slate-200 block text-xs font-medium">Sever US_EAST WAN Link</span>
                  <span className="text-[10px] text-slate-500">Simulate undersea cable cut / outage</span>
                </div>
              </div>
              <button
                onClick={() => setIsPartitioned(!isPartitioned)}
                className={`px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                  isPartitioned
                    ? 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
                    : 'bg-white/5 text-slate-400 hover:text-white border border-white/5'
                }`}
              >
                {isPartitioned ? 'SEVERED' : 'HEALTHY'}
              </button>
            </div>

            {/* Adaptive Routing Switch */}
            <div className="flex items-center justify-between p-2 rounded-lg bg-white/[0.02] border border-white/5">
              <div className="flex items-center gap-2">
                <Zap className={`w-3.5 h-3.5 ${adaptiveRoutingActive ? 'text-emerald-400' : 'text-slate-500'}`} />
                <div>
                  <span className="text-slate-200 block text-xs font-medium">WAN-Adaptive Routing Engine</span>
                  <span className="text-[10px] text-slate-500">Reroute around degraded link via AP_SOUTH</span>
                </div>
              </div>
              <button
                onClick={() => setAdaptiveRoutingActive(!adaptiveRoutingActive)}
                className={`px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                  adaptiveRoutingActive
                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/40'
                    : 'bg-white/5 text-slate-400 hover:text-white border border-white/5'
                }`}
              >
                {adaptiveRoutingActive ? 'ENABLED' : 'DISABLED'}
              </button>
            </div>

            {/* Traffic Generator Slider */}
            <div>
              <div className="flex items-center justify-between mb-1.5">
                <span className="text-slate-400 text-[11px]">Ingress Traffic Generator:</span>
                <span className="font-semibold text-slate-200 bg-white/5 px-2 py-0.5 rounded border border-white/5">
                  {trafficRateEps} eps
                </span>
              </div>
              <input
                type="range"
                min="0"
                max="40"
                step="2"
                value={trafficRateEps}
                onChange={(e) => setTrafficRateEps(Number(e.target.value))}
                className="w-full accent-purple-400 h-1 bg-white/10 rounded-lg cursor-pointer"
              />
            </div>
          </div>
        </div>

        {/* Custom Event Producer Panel */}
        <div className="bg-[#080512] border border-white/5 rounded-xl p-3.5 space-y-3">
          <div className="flex items-center gap-2 border-b border-white/5 pb-2">
            <Send className="w-3.5 h-3.5 text-cyan-400" />
            <h3 className="text-xs font-semibold font-mono uppercase text-slate-100">
              Sovereign Event Producer & Enforcer
            </h3>
          </div>

          <form onSubmit={handleInjectEvent} className="space-y-3 text-xs font-mono">
            <div>
              <label className="text-slate-400 block mb-1 text-[11px]">Stream Topic:</label>
              <input
                type="text"
                value={topic}
                onChange={(e) => setTopic(e.target.value)}
                className="w-full bg-white/[0.03] border border-white/10 rounded px-2.5 py-1.5 text-slate-200 outline-none focus:border-white/20 font-mono text-xs"
              />
            </div>

            <div className="grid grid-cols-2 gap-2">
              <div>
                <label className="text-slate-400 block mb-1 text-[11px]">Partition Key:</label>
                <input
                  type="text"
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  className="w-full bg-white/[0.03] border border-white/10 rounded px-2.5 py-1.5 text-slate-200 outline-none focus:border-white/20 font-mono text-xs"
                />
              </div>
              <div>
                <label className="text-slate-400 block mb-1 text-[11px]">Target Region:</label>
                <select
                  value={targetRegion}
                  onChange={(e) => setTargetRegion(e.target.value as 'US_EAST' | 'AP_SOUTH')}
                  className="w-full bg-white/[0.03] border border-white/10 rounded px-2.5 py-1.5 text-slate-200 outline-none focus:border-white/20 font-mono text-xs"
                >
                  <option value="US_EAST" className="bg-[#0f0c1c]">US_EAST (Remote)</option>
                  <option value="AP_SOUTH" className="bg-[#0f0c1c]">AP_SOUTH (Relay)</option>
                </select>
              </div>
            </div>

            <div>
              <label className="text-slate-400 block mb-1 text-[11px]">Residency Policy Rule:</label>
              <select
                value={policyType}
                onChange={(e) => setPolicyType(e.target.value as any)}
                className="w-full bg-white/[0.03] border border-white/10 rounded px-2.5 py-1.5 text-slate-200 outline-none focus:border-white/20 font-mono text-xs"
              >
                <option value="StrictLocal" className="bg-[#0f0c1c]">StrictLocal — Stay in EU_WEST (Blocked on egress)</option>
                <option value="RestrictedReplication" className="bg-[#0f0c1c]">RestrictedReplication — Legal whitelist only</option>
                <option value="GlobalCleartext" className="bg-[#0f0c1c]">GlobalCleartext — Unrestricted cleartext egress</option>
                <option value="SealedCiphertext" className="bg-[#0f0c1c]">SealedCiphertext — Encrypted envelope</option>
              </select>
            </div>

            {injectionFeedback && (
              <div
                className={`p-2 rounded text-[11px] font-medium ${
                  injectionFeedback.startsWith('✓')
                    ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
                    : 'bg-rose-500/10 text-rose-300 border border-rose-500/20'
                }`}
              >
                {injectionFeedback}
              </div>
            )}

            <button
              type="submit"
              className="w-full py-1.5 rounded-lg bg-white/10 hover:bg-white/15 text-white font-medium font-mono text-xs border border-white/10 transition-colors flex items-center justify-center gap-1.5"
            >
              <Shield className="w-3.5 h-3.5 text-purple-400" />
              Produce Event & Test Policy
            </button>
          </form>
        </div>
      </div>
    </div>
  );
};
