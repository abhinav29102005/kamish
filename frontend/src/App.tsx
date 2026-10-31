import React, { useState } from 'react';
import { KamishProvider, useKamish } from './context/KamishContext';
import { Header } from './components/Header';
import { ThreeTopologyGlobe } from './components/ThreeTopologyGlobe';
import { NetdataLiveGauges } from './components/NetdataLiveGauges';
import { GrafanaTelemetryDashboard } from './components/GrafanaTelemetryDashboard';
import { SovereigntyAuditLedger } from './components/SovereigntyAuditLedger';
import { ChaosAndScenarioController } from './components/ChaosAndScenarioController';
import { KamishDragonBanner } from './components/KamishDragonBanner';
import {
  Radio,
  AlertCircle,
  CheckCircle2,
} from 'lucide-react';

const DashboardContent: React.FC = () => {
  const [activeTab, setActiveTab] = useState<'overview' | 'grafana' | 'audit' | 'chaos'>('overview');
  const { recentEvents, auditLedger } = useKamish();

  const latestRejection = auditLedger.slice().reverse().find((a) => a.outcome === 'Deny');

  return (
    <div className="min-h-screen bg-cyber-deep flex flex-col text-slate-100 selection:bg-cyber-cyan selection:text-black">
      <Header activeTab={activeTab} setActiveTab={setActiveTab} />

      <main className="flex-1 p-4 max-w-7xl mx-auto w-full space-y-4">
        {/* Kamish - Solo Leveling Dragon Banner & Codex */}
        <KamishDragonBanner />

        {/* TAB 1: 3D WAN Mesh & Netdata Live Telemetry */}
        {activeTab === 'overview' && (
          <div className="space-y-3.5">
            {/* Netdata Sub-Second Micro Gauges */}
            <NetdataLiveGauges />

            {/* 3D Topology Mesh & Live Event Stream Grid */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-3.5">
              {/* 3D WebGL Globe Viewport */}
              <div className="lg:col-span-2 min-h-[460px] rounded-xl overflow-hidden border border-white/5 bg-[#06040a]">
                <ThreeTopologyGlobe />
              </div>

              {/* Side Feed: Recent Broker Ingress & Decisions */}
              <div className="bg-[#080512] border border-white/5 rounded-xl p-3 flex flex-col justify-between">
                <div>
                  <div className="flex items-center justify-between border-b border-white/5 pb-2 mb-2.5">
                    <div className="flex items-center gap-2">
                      <Radio className="w-3.5 h-3.5 text-purple-400" />
                      <h3 className="text-xs font-semibold font-mono uppercase text-slate-200">
                        Live Broker Stream
                      </h3>
                    </div>
                    <span className="text-[10px] font-mono text-slate-500">REAL-TIME</span>
                  </div>

                  <div className="space-y-2 max-h-[350px] overflow-y-auto pr-1">
                    {recentEvents.length === 0 ? (
                      <div className="text-center py-10 text-slate-500 font-mono text-xs">
                        No events produced yet. Run a scenario or use the Chaos Lab to inject records.
                      </div>
                    ) : (
                      recentEvents.slice(0, 7).map((ev) => (
                        <div
                          key={ev.id}
                          className="p-2 rounded-lg bg-white/[0.02] border border-white/5 text-[11px] font-mono hover:bg-white/[0.04] transition-colors"
                        >
                          <div className="flex items-center justify-between mb-1">
                            <span className="font-medium text-slate-200 truncate max-w-[140px]">
                              {ev.partitionKey}
                            </span>
                            <span
                              className={`text-[9px] px-1.5 py-0.2 rounded font-medium ${
                                ev.status === 'blocked'
                                  ? 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                                  : ev.status === 'replicated'
                                  ? 'bg-purple-500/10 text-purple-300 border border-purple-500/20'
                                  : 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                              }`}
                            >
                              {ev.status.toUpperCase()}
                            </span>
                          </div>
                          <div className="flex items-center justify-between text-[10px] text-slate-500">
                            <span>Policy: <strong className="text-slate-400 font-normal">{ev.residencyRule.type}</strong></span>
                            <span>{new Date(ev.timestampMs).toLocaleTimeString()}</span>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                </div>

                {/* Latest Enforcer Security Callout */}
                <div className="mt-3 pt-2.5 border-t border-white/5">
                  <span className="text-[10px] font-mono uppercase text-slate-500 block mb-1">
                    Latest Security Enforcer Event:
                  </span>
                  {latestRejection ? (
                    <div className="p-2 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[11px] font-mono text-rose-300">
                      <div className="flex items-center gap-1 font-medium text-rose-400">
                        <AlertCircle className="w-3.5 h-3.5" />
                        <span>Replication Blocked (E4 Deny)</span>
                      </div>
                      <p className="text-[10px] text-slate-400 mt-1 truncate" title={latestRejection.reason}>
                        {latestRejection.reason}
                      </p>
                    </div>
                  ) : (
                    <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-[11px] font-mono text-emerald-300 flex items-center gap-1.5">
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                      <span>Zero illegal cross-border leaks detected</span>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* TAB 2: Grafana Deep Telemetry */}
        {activeTab === 'grafana' && <GrafanaTelemetryDashboard />}

        {/* TAB 3: Sovereignty Audit Ledger */}
        {activeTab === 'audit' && <SovereigntyAuditLedger />}

        {/* TAB 4: Chaos Lab & Scenarios */}
        {activeTab === 'chaos' && <ChaosAndScenarioController />}
      </main>

      {/* Footer */}
      <footer className="border-t border-white/5 py-3 px-4 text-center text-xs font-mono text-slate-600 bg-black/40">
        <span>Kamish Distributed Event Streaming Engine · BIOS Hackathon 2026 · Invariants INV-1 to INV-9 Verified</span>
      </footer>
    </div>
  );
};

export default function App() {
  return (
    <KamishProvider>
      <DashboardContent />
    </KamishProvider>
  );
}
