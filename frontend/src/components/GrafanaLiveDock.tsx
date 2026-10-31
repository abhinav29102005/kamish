import React, { useEffect, useRef } from 'react';
import { useKamish } from '../context/KamishContext';
import { Activity, ExternalLink, Sliders } from 'lucide-react';

interface GrafanaLiveDockProps {
  onOpenFullDashboard?: () => void;
}

export const GrafanaLiveDock: React.FC<GrafanaLiveDockProps> = ({ onOpenFullDashboard }) => {
  const { telemetryHistory, currentTelemetry, nodes, isPartitioned } = useKamish();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;

    // Clear
    ctx.clearRect(0, 0, width, height);

    // Subtle minimalist grid
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
    ctx.lineWidth = 1;
    const gridY = 4;
    for (let i = 0; i <= gridY; i++) {
      const y = (height - 24) * (i / gridY) + 6;
      ctx.beginPath();
      ctx.moveTo(32, y);
      ctx.lineTo(width - 10, y);
      ctx.stroke();
    }

    if (telemetryHistory.length < 2) return;

    const allVals = telemetryHistory.flatMap((d) => [d.localCommitted, d.remoteWatermark]);
    const minVal = Math.min(...allVals) - 1;
    const maxVal = Math.max(...allVals) + 1;
    const valRange = maxVal - minVal || 1;

    const plotWidth = width - 42;
    const plotHeight = height - 30;

    const getX = (idx: number) => 32 + (idx / (telemetryHistory.length - 1)) * plotWidth;
    const getY = (val: number) => 6 + plotHeight - ((val - minVal) / valRange) * plotHeight;

    // Y labels
    ctx.fillStyle = '#64748b';
    ctx.font = '9px "JetBrains Mono", monospace';
    ctx.textAlign = 'right';
    for (let i = 0; i <= gridY; i += 2) {
      const y = (height - 24) * (i / gridY) + 6;
      const v = maxVal - (i / gridY) * valRange;
      ctx.fillText(Math.round(v).toString(), 28, y + 3);
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

    // Fill between (subtle lag shading)
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

    // Pulsating end points
    const lastIdx = telemetryHistory.length - 1;
    const lastPt = telemetryHistory[lastIdx];

    ctx.fillStyle = '#c77dff';
    ctx.beginPath();
    ctx.arc(getX(lastIdx), getY(lastPt.localCommitted), 3, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = '#00f5d4';
    ctx.beginPath();
    ctx.arc(getX(lastIdx), getY(lastPt.remoteWatermark), 3, 0, Math.PI * 2);
    ctx.fill();
  }, [telemetryHistory]);

  return (
    <div className="bg-[#080512]/85 backdrop-blur-md border border-white/10 rounded-xl p-3 flex flex-col font-mono text-xs shadow-2xl">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-white/5 pb-2 mb-2">
        <div className="flex items-center gap-1.5">
          <Activity className="w-3.5 h-3.5 text-purple-400" />
          <h4 className="text-xs font-semibold text-slate-200 uppercase tracking-wide">
            Grafana Live Telemetry
          </h4>
        </div>

        {onOpenFullDashboard && (
          <button
            onClick={onOpenFullDashboard}
            className="flex items-center gap-1 text-[10px] text-purple-300 hover:text-white px-2 py-0.5 rounded bg-white/5 hover:bg-white/10 border border-white/10 transition-colors"
            title="Open Full Grafana Dashboard"
          >
            <span>Full Dashboard</span>
            <ExternalLink className="w-2.5 h-2.5" />
          </button>
        )}
      </div>

      {/* PromQL expression tag */}
      <div className="flex items-center justify-between text-[10px] text-slate-400 mb-2">
        <span className="truncate max-w-[210px] text-slate-500">
          kamish_offset_delta&#123;topic="eu-records"&#125;
        </span>
        <span className="text-emerald-400 font-medium">LIVE 0.5s</span>
      </div>

      {/* Mini Grafana Canvas */}
      <div className="w-full h-[130px] mb-2">
        <canvas ref={canvasRef} width={340} height={130} className="w-full h-full block" />
      </div>

      {/* Legend & Stats Grid */}
      <div className="grid grid-cols-2 gap-1.5 text-[10px] pt-1.5 border-t border-white/5">
        <div className="bg-black/40 p-1.5 rounded border border-white/5">
          <span className="text-purple-300 flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-purple-400" /> Local Committed
          </span>
          <span className="text-slate-200 font-semibold text-xs mt-0.5 block">
            {nodes.EU_WEST.localCommittedOffset}
          </span>
        </div>

        <div className="bg-black/40 p-1.5 rounded border border-white/5">
          <span className="text-cyan-300 flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" /> Durable Watermark
          </span>
          <span className="text-slate-200 font-semibold text-xs mt-0.5 block">
            {nodes.US_EAST.durableWatermark}
          </span>
        </div>

        <div className="bg-black/40 p-1.5 rounded border border-white/5 col-span-2 flex items-center justify-between">
          <div className="flex items-center gap-1 text-slate-400">
            <Sliders className="w-3 h-3 text-rose-400" />
            <span>Lag:</span>
            <span className={currentTelemetry.replicationLag > 0 ? 'text-amber-400 font-bold' : 'text-emerald-400 font-bold'}>
              {currentTelemetry.replicationLag} msgs
            </span>
          </div>

          <div className="text-slate-400">
            <span>WAN RTT: </span>
            <span className="text-rose-300 font-semibold">
              {isPartitioned ? 'SEVERED' : `${currentTelemetry.wanRttMs.toFixed(1)}ms`}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
};
