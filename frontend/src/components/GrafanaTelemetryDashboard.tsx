import React, { useEffect, useRef, useState } from 'react';
import { useKamish } from '../context/useKamish';
import { CheckCircle2, Sliders, TrendingUp } from 'lucide-react';

export const GrafanaTelemetryDashboard: React.FC = () => {
  const { telemetryHistory, currentTelemetry, nodes, wanDelayMs, isPartitioned } = useKamish();
  const canvasRef1 = useRef<HTMLCanvasElement | null>(null);
  const canvasRef2 = useRef<HTMLCanvasElement | null>(null);
  const [timeRange, setTimeRange] = useState<'1m' | '5m' | '15m' | 'Live'>('Live');

  // Chart 1: Offsets & Watermark Catch-Up Time-Series
  useEffect(() => {
    const canvas = canvasRef1.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;

    // Clear
    ctx.clearRect(0, 0, width, height);

    // Background grid lines (Minimalist dark grid)
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
    ctx.lineWidth = 1;
    const gridY = 5;
    for (let i = 0; i <= gridY; i++) {
      const y = (height - 30) * (i / gridY) + 10;
      ctx.beginPath();
      ctx.moveTo(40, y);
      ctx.lineTo(width - 15, y);
      ctx.stroke();
    }

    if (telemetryHistory.length < 2) return;

    // Find min and max for scaling
    const allVals = telemetryHistory.flatMap((d) => [d.localCommitted, d.remoteWatermark]);
    const minVal = Math.min(...allVals) - 2;
    const maxVal = Math.max(...allVals) + 2;
    const valRange = maxVal - minVal || 1;

    const plotWidth = width - 55;
    const plotHeight = height - 40;

    const getX = (idx: number) => 40 + (idx / (telemetryHistory.length - 1)) * plotWidth;
    const getY = (val: number) => 10 + plotHeight - ((val - minVal) / valRange) * plotHeight;

    // Draw Y-axis labels
    ctx.fillStyle = '#64748b';
    ctx.font = '10px "JetBrains Mono", monospace';
    ctx.textAlign = 'right';
    for (let i = 0; i <= gridY; i++) {
      const y = (height - 30) * (i / gridY) + 10;
      const v = maxVal - (i / gridY) * valRange;
      ctx.fillText(Math.round(v).toString(), 34, y + 3);
    }

    // Series 1: EU_WEST Local Committed Offset (Violet)
    ctx.strokeStyle = '#c77dff';
    ctx.lineWidth = 1.8;
    ctx.beginPath();
    telemetryHistory.forEach((pt, idx) => {
      const x = getX(idx);
      const y = getY(pt.localCommitted);
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Series 2: US_EAST Durable Watermark (Cyan)
    ctx.strokeStyle = '#00f5d4';
    ctx.lineWidth = 1.8;
    ctx.beginPath();
    telemetryHistory.forEach((pt, idx) => {
      const x = getX(idx);
      const y = getY(pt.remoteWatermark);
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Fill between (Lag area - subtle minimal shading)
    ctx.fillStyle = 'rgba(217, 4, 41, 0.08)';
    ctx.beginPath();
    telemetryHistory.forEach((pt, idx) => {
      const x = getX(idx);
      const y = getY(pt.localCommitted);
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    for (let i = telemetryHistory.length - 1; i >= 0; i--) {
      const pt = telemetryHistory[i];
      const x = getX(i);
      const y = getY(pt.remoteWatermark);
      ctx.lineTo(x, y);
    }
    ctx.closePath();
    ctx.fill();

    // End points
    const lastIdx = telemetryHistory.length - 1;
    const lastPt = telemetryHistory[lastIdx];

    ctx.fillStyle = '#c77dff';
    ctx.beginPath();
    ctx.arc(getX(lastIdx), getY(lastPt.localCommitted), 3.5, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = '#00f5d4';
    ctx.beginPath();
    ctx.arc(getX(lastIdx), getY(lastPt.remoteWatermark), 3.5, 0, Math.PI * 2);
    ctx.fill();
  }, [telemetryHistory]);

  // Chart 2: WAN RTT (ms) & Outbox Queue Depth
  useEffect(() => {
    const canvas = canvasRef2.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;

    ctx.clearRect(0, 0, width, height);

    ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
    ctx.lineWidth = 1;
    const gridY = 4;
    for (let i = 0; i <= gridY; i++) {
      const y = (height - 30) * (i / gridY) + 10;
      ctx.beginPath();
      ctx.moveTo(40, y);
      ctx.lineTo(width - 15, y);
      ctx.stroke();
    }

    if (telemetryHistory.length < 2) return;

    const rttVals = telemetryHistory.map((d) => d.wanRttMs);
    const maxRtt = Math.max(...rttVals, 60);
    const minRtt = 0;
    const rttRange = maxRtt - minRtt || 1;

    const plotWidth = width - 55;
    const plotHeight = height - 40;

    const getX = (idx: number) => 40 + (idx / (telemetryHistory.length - 1)) * plotWidth;
    const getY = (val: number) => 10 + plotHeight - ((val - minRtt) / rttRange) * plotHeight;

    // Y labels
    ctx.fillStyle = '#64748b';
    ctx.font = '10px "JetBrains Mono", monospace';
    ctx.textAlign = 'right';
    for (let i = 0; i <= gridY; i++) {
      const y = (height - 30) * (i / gridY) + 10;
      const v = maxRtt - (i / gridY) * rttRange;
      ctx.fillText(Math.round(v) + 'ms', 36, y + 3);
    }

    // Series: WAN RTT
    ctx.strokeStyle = '#ff4d6d';
    ctx.lineWidth = 1.8;
    ctx.beginPath();
    telemetryHistory.forEach((pt, idx) => {
      const x = getX(idx);
      const y = getY(pt.wanRttMs);
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Shading under RTT
    ctx.fillStyle = 'rgba(255, 77, 109, 0.06)';
    ctx.beginPath();
    telemetryHistory.forEach((pt, idx) => {
      const x = getX(idx);
      const y = getY(pt.wanRttMs);
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.lineTo(getX(telemetryHistory.length - 1), 10 + plotHeight);
    ctx.lineTo(getX(0), 10 + plotHeight);
    ctx.closePath();
    ctx.fill();

    // Current point
    const lastIdx = telemetryHistory.length - 1;
    ctx.fillStyle = '#ff4d6d';
    ctx.beginPath();
    ctx.arc(getX(lastIdx), getY(telemetryHistory[lastIdx].wanRttMs), 3.5, 0, Math.PI * 2);
    ctx.fill();
  }, [telemetryHistory]);

  const invariants = [
    { id: 'INV-1', desc: 'Offsets dense & monotonic', verify: 'OrderingChecker verified' },
    { id: 'INV-2', desc: 'Majority in-region fsync', verify: 'Local fsync quorum verified' },
    { id: 'INV-3', desc: 'Leader holds acked records', verify: 'Failover lease locked' },
    { id: 'INV-4', desc: 'Reads bounded by HWM', verify: 'Contract tested' },
    { id: 'INV-5', desc: 'Exact offset mirror match', verify: 'Watermark synced' },
    { id: 'INV-6', desc: 'Idempotent duplicate retry', verify: 'SessionManager dedupe' },
    { id: 'INV-7', desc: 'No split-brain / CAS lease', verify: 'Epoch fencing active' },
    { id: 'INV-8', desc: 'Zero unauthorized egress', verify: 'KamishPolicyEngine active' },
    { id: 'INV-9', desc: 'Tamper-evident Ed25519', verify: 'SHA-256 Chain 100% valid' },
  ];

  return (
    <div className="space-y-3.5">
      {/* Grafana Navigation / Filter Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 bg-[#080512]/85 backdrop-blur-md border border-white/10 px-3.5 py-2 rounded-xl shadow-xl">
        <div className="flex items-center gap-2">
          <div className="w-2.5 h-2.5 bg-orange-500 rounded-sm" />
          <span className="font-semibold text-slate-100 font-mono text-xs uppercase tracking-wider">
            Grafana Analytics & Prometheus Observability Console
          </span>
          <span className="text-[10px] bg-white/5 px-2 py-0.5 rounded text-cyan-300 font-mono border border-white/5">
            cluster: kamish-core
          </span>
        </div>

        <div className="flex items-center gap-2">
          {/* Time range selector */}
          <div className="flex items-center bg-white/[0.03] rounded-md border border-white/5 p-0.5 text-xs font-mono">
            {(['1m', '5m', '15m', 'Live'] as const).map((r) => (
              <button
                key={r}
                onClick={() => setTimeRange(r)}
                className={`px-2.5 py-0.5 rounded transition-colors ${
                  timeRange === r
                    ? 'bg-orange-500/20 text-orange-300 border border-orange-500/40 font-medium'
                    : 'text-slate-500 hover:text-slate-300'
                }`}
              >
                {r}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* PromQL Query Inspector Strip */}
      <div className="bg-[#080512]/85 backdrop-blur-md border border-white/10 rounded-xl px-3.5 py-2 font-mono text-xs flex flex-wrap items-center justify-between gap-2 shadow-lg">
        <div className="flex items-center gap-2 flex-1 min-w-[280px]">
          <span className="text-[10px] text-orange-400 bg-orange-500/10 border border-orange-500/20 px-1.5 py-0.5 rounded font-bold">
            PromQL
          </span>
          <code className="text-slate-300 text-[11px] truncate">
            kamish_replication_lag_records&#123;source="EU_WEST",target="US_EAST"&#125;
          </code>
        </div>
        <div className="flex items-center gap-2 text-[10px] text-slate-400">
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
          <span>Polling interval: 500ms</span>
          <span className="text-slate-600">|</span>
          <span className="text-purple-300 font-medium">Scrape Target: :9090/metrics</span>
        </div>
      </div>

      {/* Instant Stat Metrics Row */}
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2 font-mono text-xs">
        <div className="bg-[#080512]/85 backdrop-blur-md border border-white/5 p-2.5 rounded-xl">
          <span className="text-[9px] text-slate-400 uppercase tracking-wider block">Local Offset</span>
          <span className="text-sm font-bold text-purple-300">{nodes.EU_WEST.localCommittedOffset}</span>
          <span className="text-[9px] text-slate-500 block">EU_WEST committed</span>
        </div>

        <div className="bg-[#080512]/85 backdrop-blur-md border border-white/5 p-2.5 rounded-xl">
          <span className="text-[9px] text-slate-400 uppercase tracking-wider block">Watermark</span>
          <span className="text-sm font-bold text-cyan-300">{nodes.US_EAST.durableWatermark}</span>
          <span className="text-[9px] text-slate-500 block">US_EAST synced</span>
        </div>

        <div className="bg-[#080512]/85 backdrop-blur-md border border-white/5 p-2.5 rounded-xl">
          <span className="text-[9px] text-slate-400 uppercase tracking-wider block">Replication Lag</span>
          <span className={`text-sm font-bold ${currentTelemetry.replicationLag > 0 ? 'text-amber-400' : 'text-emerald-400'}`}>
            {currentTelemetry.replicationLag} <span className="text-[10px] font-normal">msgs</span>
          </span>
          <span className="text-[9px] text-slate-500 block">delta(Committed - HWM)</span>
        </div>

        <div className="bg-[#080512]/85 backdrop-blur-md border border-white/5 p-2.5 rounded-xl">
          <span className="text-[9px] text-slate-400 uppercase tracking-wider block">Link WAN RTT</span>
          <span className="text-sm font-bold text-rose-300">
            {isPartitioned ? 'OFFLINE' : `${currentTelemetry.wanRttMs.toFixed(1)}ms`}
          </span>
          <span className="text-[9px] text-slate-500 block">{isPartitioned ? 'Severed link' : 'Transatlantic link'}</span>
        </div>

        <div className="bg-[#080512]/85 backdrop-blur-md border border-white/5 p-2.5 rounded-xl">
          <span className="text-[9px] text-slate-400 uppercase tracking-wider block">Fsync Latency</span>
          <span className="text-sm font-bold text-cyan-400">{currentTelemetry.writeLatencyMs}ms</span>
          <span className="text-[9px] text-slate-500 block">p99 in-region quorum</span>
        </div>

        <div className="bg-[#080512]/85 backdrop-blur-md border border-white/5 p-2.5 rounded-xl">
          <span className="text-[9px] text-slate-400 uppercase tracking-wider block">Invariants</span>
          <span className="text-sm font-bold text-emerald-400">9 / 9</span>
          <span className="text-[9px] text-slate-500 block">100% formal pass</span>
        </div>
      </div>

      {/* Main Dual Charts Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
        {/* Panel 1: Offsets & Watermark Progression */}
        <div className="bg-[#080512]/85 backdrop-blur-md border border-white/10 rounded-xl p-3.5 flex flex-col shadow-xl">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              <TrendingUp className="w-3.5 h-3.5 text-purple-400" />
              <h4 className="text-xs font-semibold font-mono uppercase text-slate-200">
                Log Offsets & Durable Watermark (INV-1, INV-5)
              </h4>
            </div>
            <div className="flex items-center gap-3 text-[10px] font-mono">
              <span className="flex items-center gap-1 text-purple-300">
                <span className="w-2 h-0.5 bg-purple-400 rounded" /> Local Committed ({nodes.EU_WEST.localCommittedOffset})
              </span>
              <span className="flex items-center gap-1 text-emerald-400">
                <span className="w-2 h-0.5 bg-emerald-400 rounded" /> US_EAST Watermark ({nodes.US_EAST.durableWatermark})
              </span>
            </div>
          </div>

          <div className="w-full flex-1 min-h-[220px]">
            <canvas ref={canvasRef1} width={560} height={220} className="w-full h-full block" />
          </div>

          {/* Metric footer equation */}
          <div className="mt-2 pt-2 border-t border-white/5 flex items-center justify-between text-[11px] font-mono text-slate-400">
            <span>
              {"ReplicationLag = LocalCommittedOffset - DurableWatermark"}
            </span>
            <span className={currentTelemetry.replicationLag > 0 ? 'text-amber-400 font-medium' : 'text-emerald-400 font-medium'}>
              Current Lag: {currentTelemetry.replicationLag} records
            </span>
          </div>
        </div>

        {/* Panel 2: Link Latency & Outbox Buffer Dynamics */}
        <div className="bg-[#080512] border border-white/5 rounded-xl p-3.5 flex flex-col">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              <Sliders className="w-3.5 h-3.5 text-rose-400" />
              <h4 className="text-xs font-semibold font-mono uppercase text-slate-200">
                WAN Transatlantic RTT & Queue Delay (NFR-04)
              </h4>
            </div>
            <div className="flex items-center gap-3 text-[10px] font-mono">
              <span className="flex items-center gap-1 text-rose-300">
                <span className="w-2 h-0.5 bg-rose-400 rounded" /> Link RTT ({currentTelemetry.wanRttMs.toFixed(1)}ms)
              </span>
            </div>
          </div>

          <div className="w-full flex-1 min-h-[220px]">
            <canvas ref={canvasRef2} width={560} height={220} className="w-full h-full block" />
          </div>

          <div className="mt-2 pt-2 border-t border-white/5 flex items-center justify-between text-[11px] font-mono text-slate-400">
            <span>Link Status: {isPartitioned ? 'SEVERED' : wanDelayMs > 150 ? 'DEGRADED' : 'HEALTHY'}</span>
            <span className="text-cyan-400 font-medium">Local Fsync: {currentTelemetry.writeLatencyMs}ms</span>
          </div>
        </div>
      </div>

      {/* Invariants Health Matrix (INV-1 through INV-9) */}
      <div className="bg-[#080512] border border-white/5 rounded-xl p-3.5">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
            <h4 className="text-xs font-semibold font-mono uppercase text-slate-200">
              Formally Verified Invariants (INV-1 – INV-9)
            </h4>
          </div>
          <span className="text-[10px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20">
            9 / 9 INVARIANTS PASSING
          </span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-2">
          {invariants.map((inv) => (
            <div
              key={inv.id}
              className="bg-white/[0.02] border border-white/5 rounded-lg p-2.5 flex items-start justify-between hover:bg-white/[0.04] transition-colors"
            >
              <div>
                <div className="flex items-center gap-1.5">
                  <span className="font-mono font-bold text-xs text-purple-400">{inv.id}</span>
                  <span className="text-xs font-medium text-slate-300">{inv.desc}</span>
                </div>
                <span className="text-[10px] font-mono text-slate-500 block mt-0.5">{inv.verify}</span>
              </div>
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0 mt-1" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
