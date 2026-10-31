import React, { useState } from 'react';
import { useKamish } from '../context/useKamish';
import { Flame, ShieldAlert, Sparkles, ChevronDown, ChevronUp, Zap, Swords } from 'lucide-react';

export const KamishDragonBanner: React.FC = () => {
  const { isShadowMonarchMode, setIsShadowMonarchMode, runScenario } = useKamish();
  const [isOpen, setIsOpen] = useState<boolean>(false);

  return (
    <div
      className={`rounded-xl border transition-all duration-300 overflow-hidden ${
        isShadowMonarchMode
          ? 'bg-cyber-panel/90 border-purple-500/30'
          : 'bg-cyber-panel/90 border-cyber-border/70'
      }`}
    >
      {/* Sleek Minimalist Ribbon */}
      <div className="flex flex-wrap items-center justify-between px-3.5 py-2 bg-black/40">
        <div className="flex items-center gap-2">
          <span className="text-base">🐉</span>
          <div className="flex items-center gap-2 font-mono text-xs">
            <span className="font-bold text-white tracking-wide">KAMISH</span>
            <span className="text-slate-500 font-normal">/</span>
            <span className="text-purple-300 text-[11px]">Humanity's Greatest Calamity</span>
            <span className="text-[9px] px-1.5 py-0.2 rounded bg-purple-500/15 text-purple-300 border border-purple-500/30 font-semibold hidden sm:inline">
              S-RANK BROKER
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={() => setIsShadowMonarchMode(!isShadowMonarchMode)}
            className={`flex items-center gap-1 px-2.5 py-1 rounded text-[11px] font-mono transition-colors ${
              isShadowMonarchMode
                ? 'bg-purple-600/80 text-white hover:bg-purple-500'
                : 'bg-black/40 text-slate-400 hover:text-white border border-white/5'
            }`}
          >
            <Sparkles className="w-3 h-3" />
            <span>{isShadowMonarchMode ? 'Shadow Mode: ON' : 'Shadow Mode'}</span>
          </button>

          <button
            onClick={() => setIsOpen(!isOpen)}
            className="flex items-center gap-1 px-2 py-1 rounded bg-black/40 text-slate-400 hover:text-slate-200 text-[11px] font-mono border border-white/5 transition-colors"
            title="Toggle Dragon Lore Details"
          >
            <span>{isOpen ? 'Close Codex' : 'Lore Codex'}</span>
            {isOpen ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
          </button>
        </div>
      </div>

      {/* Expandable Lore & Architecture Mapping */}
      {isOpen && (
        <div className="p-4 space-y-3 font-mono text-xs">
          <div className="text-[11px] text-slate-300 leading-relaxed font-sans">
            In tribute to <strong className="text-purple-300">Kamish</strong>, the legendary ancient dragon whose brute force decimated entire battlefields before being summoned by the Shadow Monarch, our event broker incorporates his mythical powers into distributed system invariants:
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-2.5">
              <div className="p-2 rounded-lg bg-black/50 border border-purple-500/30 flex items-start gap-2">
                <ShieldAlert className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
                <div>
                  <strong className="text-purple-300 block text-[11px]">Dragon's Fear (Enforcer E1–E10)</strong>
                  <span className="text-[10px] text-slate-400 block">
                    Paralyzes illegal egress; forces data residency under strict sovereignty shields.
                  </span>
                </div>
              </div>

              <div className="p-2 rounded-lg bg-black/50 border border-purple-500/30 flex items-start gap-2">
                <Flame className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
                <div>
                  <strong className="text-amber-300 block text-[11px]">Dragon's Breath (WAN Streams)</strong>
                  <span className="text-[10px] text-slate-400 block">
                    High-throughput event replication cutting through degraded transatlantic WAN links.
                  </span>
                </div>
              </div>

              <div className="p-2 rounded-lg bg-black/50 border border-purple-500/30 flex items-start gap-2">
                <Swords className="w-4 h-4 text-cyber-cyan shrink-0 mt-0.5" />
                <div>
                  <strong className="text-cyan-300 block text-[11px]">Kamish's Wrath (Cryptographic Fangs)</strong>
                  <span className="text-[10px] text-slate-400 block">
                    Unbreakable SHA-256 hash chains & Ed25519 signatures preventing policy rollback.
                  </span>
                </div>
              </div>

              <div className="p-2 rounded-lg bg-black/50 border border-purple-500/30 flex items-start gap-2">
                <Zap className="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
                <div>
                  <strong className="text-emerald-300 block text-[11px]">Shadow Resurrection (Zero Data Loss)</strong>
                  <span className="text-[10px] text-slate-400 block">
                    Resurrects remote state from durable watermarks following catastrophic regional collapse.
                  </span>
                </div>
              </div>
            </div>

            {/* Quick Lore Scenario Triggers */}
            <div className="pt-1 flex flex-wrap items-center gap-2">
              <button
                onClick={() => runScenario('A')}
                className="px-2.5 py-1 rounded bg-rose-500/20 text-rose-300 border border-rose-500/40 hover:bg-rose-500/30 text-[10px] font-bold transition-colors flex items-center gap-1"
              >
                <ShieldAlert className="w-3 h-3" />
                <span>Test Dragon's Fear (Residency Block)</span>
              </button>

              <button
                onClick={() => runScenario('C')}
                className="px-2.5 py-1 rounded bg-purple-500/20 text-purple-300 border border-purple-500/40 hover:bg-purple-500/30 text-[10px] font-bold transition-colors flex items-center gap-1"
              >
                <Zap className="w-3 h-3" />
                <span>Test Shadow Resurrection (Watermark Recovery)</span>
              </button>
            </div>
        </div>
      )}
    </div>
  );
};
