import React, { useEffect, useState } from 'react';
import { useKamish } from '../context/KamishContext';
import {
  Globe,
  Activity,
  ShieldCheck,
  Zap,
  Radio,
  Play,
  Sparkles,
  Flame,
} from 'lucide-react';

interface HeaderProps {
  activeTab: 'overview' | 'grafana' | 'audit' | 'chaos';
  setActiveTab: (tab: 'overview' | 'grafana' | 'audit' | 'chaos') => void;
}

export const Header: React.FC<HeaderProps> = ({ activeTab, setActiveTab }) => {
  const { nodes, runScenario, scenarioState, isShadowMonarchMode, setIsShadowMonarchMode } = useKamish();
  const [utcTime, setUtcTime] = useState<string>('');

  useEffect(() => {
    const updateTime = () => {
      const now = new Date();
      setUtcTime(now.toUTCString().replace('GMT', 'UTC'));
    };
    updateTime();
    const interval = setInterval(updateTime, 1000);
    return () => clearInterval(interval);
  }, []);

  return (
    <header
      className={`border-b transition-colors duration-300 backdrop-blur-md sticky top-0 z-50 ${
        isShadowMonarchMode
          ? 'bg-[#080511]/95 border-purple-500/20'
          : 'bg-[#06040a]/95 border-white/10'
      }`}
    >
      {/* Top Telemetry Ticker - Minimalist single line */}
      <div className="bg-black/40 px-4 py-1 border-b border-white/5 flex flex-wrap items-center justify-between text-[11px] font-mono text-slate-400">
        <div className="flex items-center gap-3">
          <span className="flex items-center gap-1.5 text-slate-200">
            <Radio className="w-3 h-3 text-purple-400" />
            <span className="text-slate-400">CLUSTER:</span> KAMISH
          </span>
          <span className="text-white/10">·</span>
          <span className="flex items-center gap-2">
            <span>EPOCH {nodes.EU_WEST.activeEpoch}</span>
            <span className="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] bg-black/50 border border-white/5 font-mono">
              <span
                className={`w-1.5 h-1.5 rounded-full ${
                  nodes.EU_WEST.leaseExpiresInSec <= 1
                    ? 'bg-rose-500'
                    : nodes.EU_WEST.leaseExpiresInSec <= 3
                    ? 'bg-amber-400'
                    : 'bg-emerald-400'
                }`}
              />
              <span className="text-slate-500">LEASE</span>
              <span
                className={`font-semibold tabular-nums ${
                  nodes.EU_WEST.leaseExpiresInSec <= 1
                    ? 'text-rose-400'
                    : nodes.EU_WEST.leaseExpiresInSec <= 3
                    ? 'text-amber-400'
                    : 'text-emerald-400'
                }`}
              >
                {nodes.EU_WEST.leaseExpiresInSec.toFixed(1)}s
              </span>
            </span>
          </span>
          <span className="hidden sm:inline text-white/10">·</span>
          <span className="hidden sm:inline text-slate-400">
            3/3 Nodes Online
          </span>
          <span className="hidden md:inline text-white/10">·</span>
          <span className="hidden md:inline text-emerald-400">
            INV 9/9 Pass
          </span>
        </div>

        <div className="flex items-center gap-3 text-[10px]">
          <span className="text-slate-500">{utcTime}</span>
          <span className="text-purple-300 bg-purple-950/40 border border-purple-500/20 px-2 py-0.5 rounded font-mono">
            {isShadowMonarchMode ? 'SHADOW' : 'STANDBY'}
          </span>
        </div>
      </div>

      {/* Main Header & Nav */}
      <div className="px-4 py-2.5 flex flex-wrap items-center justify-between gap-3">
        {/* Minimalist Logo */}
        <div className="flex items-center space-x-2.5">
          <div className="w-8 h-8 rounded-lg bg-black/60 border border-purple-500/30 flex items-center justify-center text-sm">
            🐉
          </div>
          <div>
            <div className="flex items-center gap-1.5">
              <span className="text-sm font-bold font-mono tracking-wider uppercase text-white">
                KAMISH
              </span>
              <span className="text-[10px] text-slate-500 font-mono">/ BROKER</span>
            </div>
            <p className="text-[10px] text-slate-400 font-mono tracking-tight hidden sm:block">
              Sovereignty-Aware WAN Stream Engine
            </p>
          </div>
        </div>

        {/* Minimalist Tab Switcher */}
        <div className="flex items-center bg-black/50 p-1 rounded-lg border border-white/5 space-x-1">
          <button
            onClick={() => setActiveTab('overview')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono transition-colors ${
              activeTab === 'overview'
                ? isShadowMonarchMode
                  ? 'bg-purple-600 text-white font-bold'
                  : 'bg-white/10 text-white font-bold'
                : 'text-slate-400 hover:text-white'
            }`}
          >
            <Globe className="w-3.5 h-3.5" />
            <span>3D Mesh</span>
          </button>

          <button
            onClick={() => setActiveTab('grafana')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono transition-colors ${
              activeTab === 'grafana'
                ? isShadowMonarchMode
                  ? 'bg-purple-600 text-white font-bold'
                  : 'bg-white/10 text-white font-bold'
                : 'text-slate-400 hover:text-white'
            }`}
          >
            <Activity className="w-3.5 h-3.5" />
            <span>Telemetry</span>
          </button>

          <button
            onClick={() => setActiveTab('audit')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono transition-colors ${
              activeTab === 'audit'
                ? isShadowMonarchMode
                  ? 'bg-purple-600 text-white font-bold'
                  : 'bg-white/10 text-white font-bold'
                : 'text-slate-400 hover:text-white'
            }`}
          >
            <ShieldCheck className="w-3.5 h-3.5" />
            <span>Audit Ledger</span>
          </button>

          <button
            onClick={() => setActiveTab('chaos')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono transition-colors ${
              activeTab === 'chaos'
                ? isShadowMonarchMode
                  ? 'bg-purple-600 text-white font-bold'
                  : 'bg-white/10 text-white font-bold'
                : 'text-slate-400 hover:text-white'
            }`}
          >
            <Zap className="w-3.5 h-3.5" />
            <span>Chaos Lab</span>
          </button>
        </div>

        {/* Minimalist Action Controls */}
        <div className="flex items-center gap-2 font-mono text-xs">
          <button
            onClick={() => runScenario('A')}
            disabled={scenarioState.status === 'running'}
            className="hidden xl:flex items-center gap-1 px-2.5 py-1 rounded bg-black/40 text-slate-300 border border-white/5 hover:border-white/20 transition-colors disabled:opacity-50 text-[11px]"
            title="Gate 2 / Demo A: Residency Enforcement"
          >
            <Play className="w-3 h-3 fill-current text-slate-400" />
            <span>Demo A</span>
          </button>

          <button
            onClick={() => runScenario('B')}
            disabled={scenarioState.status === 'running'}
            className="hidden xl:flex items-center gap-1 px-2.5 py-1 rounded bg-black/40 text-slate-300 border border-white/5 hover:border-white/20 transition-colors disabled:opacity-50 text-[11px]"
            title="Gate 3 / Demo B: Slow-Link & Lag Metrics"
          >
            <Play className="w-3 h-3 fill-current text-slate-400" />
            <span>Demo B</span>
          </button>

          {/* Minimalist ARISE button */}
          <button
            onClick={() => runScenario('C')}
            disabled={scenarioState.status === 'running'}
            className="flex items-center gap-1.5 px-3 py-1 rounded bg-purple-600 text-white hover:bg-purple-500 transition-colors font-bold uppercase tracking-wide text-xs"
            title="Sung Jin-Woo's Command: Arise! Resurrects failed region and catches up watermarks"
          >
            <Sparkles className="w-3.5 h-3.5" />
            <span>ARISE</span>
          </button>

          <button
            onClick={() => setIsShadowMonarchMode(!isShadowMonarchMode)}
            className="p-1 rounded bg-black/40 border border-white/5 text-slate-400 hover:text-purple-300 transition-colors"
            title="Toggle Shadow Monarch Theme"
          >
            <Flame className="w-3.5 h-3.5 fill-current text-purple-400" />
          </button>
        </div>
      </div>
    </header>
  );
};
