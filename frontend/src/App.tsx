import React, { useState } from 'react';
import { KamishProvider, useKamish } from './context/KamishContext';
import { Header } from './components/Header';
import { ThreeTopologyGlobe } from './components/ThreeTopologyGlobe';
import { NetdataLiveGauges } from './components/NetdataLiveGauges';
import { GrafanaTelemetryDashboard } from './components/GrafanaTelemetryDashboard';
import { GrafanaLiveDock } from './components/GrafanaLiveDock';
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
  const [showHud, setShowHud] = useState<boolean>(true);
  const { recentEvents, auditLedger, isShadowMonarchMode } = useKamish();

  const latestRejection = auditLedger.slice().reverse().find((a) => a.outcome === 'Deny');

  return (
    <div className="relative min-h-screen text-slate-100 selection:bg-purple-500 selection:text-white overflow-x-hidden bg-[#040308]">
      {/* 1. Global Interactive 3D WebGL Canvas (The Whole Website 3D Render) */}
      <ThreeTopologyGlobe
        isGlobalBackground={true}
        activeTab={activeTab}
        showHud={showHud}
        setShowHud={setShowHud}
      />

      {/* 3. Floating Minimalist UI / HUD Layer */}
      <div className="relative z-10 min-h-screen flex flex-col pointer-events-none">
        <div className="pointer-events-auto">
          <Header
            activeTab={activeTab}
            setActiveTab={setActiveTab}
            showHud={showHud}
            setShowHud={setShowHud}
          />
        </div>

        <main className="flex-1 p-4 max-w-7xl mx-auto w-full space-y-4">
          {/* Kamish Dragon Banner & Architecture Codex */}
          {showHud && (
            <div className="pointer-events-auto">
              <KamishDragonBanner />
            </div>
          )}

          {/* TAB 1: 3D WAN Mesh & Live Observability */}
          {activeTab === 'overview' && (
            <>
              {showHud && (
                <div className="space-y-3.5">
                  {/* Netdata Sub-Second Micro Gauges floating strip */}
                  <div className="pointer-events-auto shadow-2xl rounded-xl overflow-hidden bg-[#080512]/80 backdrop-blur-md border border-white/5">
                    <NetdataLiveGauges />
                  </div>

                  {/* Main 3D Space Grid: Center is clear 3D globe, Right is floating Grafana Dock + Stream */}
                  <div className="grid grid-cols-1 lg:grid-cols-12 gap-3.5 min-h-[460px]">
                    {/* Transparent column to give 3D interactive globe ample breathing room */}
                    <div className="lg:col-span-8 min-h-[260px] pointer-events-none" />

                    {/* Right Floating Observability Column */}
                    <div className="lg:col-span-4 space-y-3 pointer-events-auto">
                      {/* Grafana Live Telemetry Dock */}
                      <GrafanaLiveDock onOpenFullDashboard={() => setActiveTab('grafana')} />

                      {/* Side Feed: Recent Broker Ingress & Decisions */}
                      <div className="bg-[#080512]/85 backdrop-blur-md border border-white/5 rounded-xl p-3 flex flex-col justify-between shadow-xl">
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

                          <div className="space-y-2 max-h-[220px] overflow-y-auto pr-1">
                            {recentEvents.length === 0 ? (
                              <div className="text-center py-6 text-slate-500 font-mono text-xs">
                                No events produced yet. Run a scenario or use the Chaos Lab to inject records.
                              </div>
                            ) : (
                              recentEvents.slice(0, 5).map((ev) => (
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
                        <div className="mt-2.5 pt-2 border-t border-white/5">
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
                </div>
              )}
            </>
          )}

          {/* TAB 2: Grafana Deep Telemetry */}
          {activeTab === 'grafana' && (
            <div className="pointer-events-auto">
              <GrafanaTelemetryDashboard />
            </div>
          )}

          {/* TAB 3: Sovereignty Audit Ledger */}
          {activeTab === 'audit' && (
            <div className="pointer-events-auto">
              <SovereigntyAuditLedger />
            </div>
          )}

          {/* TAB 4: Chaos Lab & Scenarios */}
          {activeTab === 'chaos' && (
            <div className="pointer-events-auto">
              <ChaosAndScenarioController />
            </div>
          )}
        </main>

        {/* Floating Minimalist Footer */}
        <footer className="pointer-events-auto border-t border-white/5 py-2.5 px-4 text-center text-xs font-mono text-slate-500 bg-black/60 backdrop-blur-md">
          <span>Kamish Distributed Event Streaming Engine · BIOS Hackathon 2026 · Invariants INV-1 to INV-9 Formally Verified</span>
        </footer>
      </div>
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

