import React from 'react';
import { useKamish } from '../context/KamishContext';
import { Activity, Clock, Database, Gauge, HardDrive, Zap, ShieldCheck } from 'lucide-react';

interface GaugeProps {
  label: string;
  value: number;
  unit: string;
  min: number;
  max: number;
  thresholds: { yellow: number; red: number };
  icon: React.ReactNode;
  sparklineData: number[];
  color?: string;
  decimals?: number;
  subLabel?: string;
  isShadowMode?: boolean;
  reverseThresholds?: boolean;
}

const CircularGauge: React.FC<GaugeProps> = ({
  label,
  value,
  unit,
  min,
  max,
  thresholds,
  icon,
  sparklineData,
  color,
  decimals = 0,
  subLabel,
  isShadowMode = false,
  reverseThresholds = false,
}) => {
  const clamped = Math.min(Math.max(value, min), max);
  const ratio = (clamped - min) / (max - min || 1);
  const totalAngle = 240;
  const startAngle = 150;
  const currentAngle = startAngle + ratio * totalAngle;

  const radius = 38;
  const cx = 50;
  const cy = 50;

  const polarToCartesian = (centerX: number, centerY: number, r: number, angleInDegrees: number) => {
    const angleInRadians = ((angleInDegrees - 90) * Math.PI) / 180.0;
    return {
      x: centerX + r * Math.cos(angleInRadians),
      y: centerY + r * Math.sin(angleInRadians),
    };
  };

  const describeArc = (x: number, y: number, r: number, startA: number, endA: number) => {
    const start = polarToCartesian(x, y, r, endA);
    const end = polarToCartesian(x, y, r, startA);
    const largeArcFlag = endA - startA <= 180 ? '0' : '1';
    return ['M', start.x, start.y, 'A', r, r, 0, largeArcFlag, 0, end.x, end.y].join(' ');
  };

  // Determine dynamic color (respecting reverse countdown thresholds)
  let strokeColor = color;
  if (reverseThresholds) {
    if (value <= thresholds.red) {
      strokeColor = '#d90429';
    } else if (value <= thresholds.yellow) {
      strokeColor = '#ffb703';
    } else {
      strokeColor = isShadowMode ? '#c77dff' : '#00f5d4';
    }
  } else if (!color) {
    if (value >= thresholds.red) {
      strokeColor = '#d90429';
    } else if (value >= thresholds.yellow) {
      strokeColor = '#ffb703';
    } else {
      strokeColor = isShadowMode ? '#c77dff' : '#00f5d4';
    }
  }

  // Mini sparkline SVG points
  const sparkMax = Math.max(...sparklineData, max * 0.2, 1);
  const sparkMin = Math.min(...sparklineData, min);
  const sparkPoints = sparklineData
    .map((val, idx) => {
      const x = (idx / (sparklineData.length - 1 || 1)) * 90 + 5;
      const normalizedY = (val - sparkMin) / (sparkMax - sparkMin || 1);
      const y = 25 - normalizedY * 20;
      return `${x},${y}`;
    })
    .join(' ');

  return (
    <div
      className={`relative flex flex-col items-center justify-between p-3 rounded-xl border transition-all duration-200 group ${
        isShadowMode
          ? 'bg-[#0a0714] border-purple-500/20 hover:border-purple-500/40'
          : 'bg-[#07050d] border-white/5 hover:border-white/15'
      }`}
    >
      {/* Gauge Header */}
      <div className="w-full flex items-center justify-between text-[10px] font-mono text-slate-400 mb-1">
        <div className="flex items-center gap-1.5">
          <span style={{ color: strokeColor }}>{icon}</span>
          <span className="font-semibold text-slate-300 tracking-wider uppercase text-[10px]">{label}</span>
        </div>
      </div>

      {/* Modern Minimalist Radial Meter */}
      <div className="relative w-24 h-24 flex items-center justify-center my-1">
        <svg viewBox="0 0 100 100" className="w-full h-full">
          {/* Subtle Background Track */}
          <path
            d={describeArc(cx, cy, radius, startAngle, startAngle + totalAngle)}
            fill="none"
            stroke="#1b172a"
            strokeWidth="3"
            strokeLinecap="round"
          />

          {/* Active Solid Meter Arc */}
          <path
            d={describeArc(cx, cy, radius, startAngle, Math.max(startAngle + 0.1, currentAngle))}
            fill="none"
            stroke={strokeColor}
            strokeWidth="3.5"
            strokeLinecap="round"
            className="transition-all duration-300 ease-out"
          />
        </svg>

        {/* Center Digital Readout */}
        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none mt-4">
          <span className="text-xl font-bold font-mono tracking-tight text-white">
            {value.toFixed(decimals)}
          </span>
          <span className="text-[9px] font-mono uppercase text-slate-400 font-medium tracking-wider">
            {unit}
          </span>
        </div>
      </div>

      {/* Sub-label status */}
      {subLabel && (
        <span className="text-[9px] font-mono text-slate-400 mb-1 text-center truncate max-w-full">
          {subLabel}
        </span>
      )}

      {/* Minimalist Vector Sparkline */}
      <div className="w-full h-5 px-1 mt-0.5">
        <svg viewBox="0 0 100 28" className="w-full h-full overflow-visible">
          {sparklineData.length > 1 && (
            <>
              <polygon
                points={`5,28 ${sparkPoints} 95,28`}
                fill={strokeColor}
                fillOpacity="0.08"
              />
              <polyline
                points={sparkPoints}
                fill="none"
                stroke={strokeColor}
                strokeWidth="1.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </>
          )}
        </svg>
      </div>
    </div>
  );
};

export const NetdataLiveGauges: React.FC = () => {
  const { currentTelemetry, telemetryHistory, isShadowMonarchMode, nodes } = useKamish();

  const lagHistory = telemetryHistory.map((t) => t.replicationLag);
  const outboxHistory = telemetryHistory.map((t) => t.outboxDepth);
  const rttHistory = telemetryHistory.map((t) => t.wanRttMs);
  const epsHistory = telemetryHistory.map((t) => t.ingressEps);
  const latHistory = telemetryHistory.map((t) => t.writeLatencyMs);
  const quorumLeaseHistory = telemetryHistory.map((t) => t.leaseExpiresInSec ?? nodes.EU_WEST.leaseExpiresInSec);

  return (
    <div className="w-full">
      {/* Minimalist Title bar */}
      <div className="flex items-center justify-between mb-2 px-1 text-xs font-mono">
        <div className="flex items-center gap-2">
          <Activity className="w-3.5 h-3.5 text-purple-400" />
          <span className="font-semibold tracking-wider uppercase text-slate-300">
            Netdata Telemetry
          </span>
          <span className="text-[10px] text-slate-500 font-normal">100ms stream</span>
        </div>
        <div className="flex items-center gap-2 text-[11px] text-slate-400">
          <span>Drift Safe:</span>
          <span className="text-emerald-400 font-semibold">{nodes.EU_WEST.leaseExpiresInSec.toFixed(1)}s</span>
        </div>
      </div>

      {/* 6 Real-Time Responsive Gauges Grid */}
      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
        {/* 1. Replication Lag */}
        <CircularGauge
          label="Replication Lag"
          value={currentTelemetry.replicationLag}
          unit="offsets"
          min={0}
          max={60}
          thresholds={{ yellow: 15, red: 40 }}
          icon={<Gauge className="w-3.5 h-3.5" />}
          sparklineData={lagHistory}
          subLabel={currentTelemetry.replicationLag === 0 ? 'Watermarks In Sync' : 'Replication Delay'}
          isShadowMode={isShadowMonarchMode}
        />

        {/* 2. Outbox Buffer Capacity */}
        <CircularGauge
          label="Outbox Queue"
          value={currentTelemetry.outboxDepth}
          unit="msgs"
          min={0}
          max={60}
          thresholds={{ yellow: 20, red: 45 }}
          icon={<HardDrive className="w-3.5 h-3.5" />}
          sparklineData={outboxHistory}
          subLabel="Bounded Disk Buffer"
          isShadowMode={isShadowMonarchMode}
        />

        {/* 3. WAN Link RTT */}
        <CircularGauge
          label="Link Latency"
          value={currentTelemetry.wanRttMs}
          unit="ms"
          min={0}
          max={400}
          thresholds={{ yellow: 120, red: 280 }}
          icon={<Clock className="w-3.5 h-3.5" />}
          sparklineData={rttHistory}
          decimals={1}
          subLabel="Transatlantic RTT"
          isShadowMode={isShadowMonarchMode}
        />

        {/* 4. Local Write Fsync Latency */}
        <CircularGauge
          label="Local Fsync"
          value={currentTelemetry.writeLatencyMs}
          unit="ms"
          min={0}
          max={5}
          thresholds={{ yellow: 1.5, red: 3.0 }}
          icon={<Zap className="w-3.5 h-3.5" />}
          sparklineData={latHistory}
          color="#00e676"
          decimals={2}
          subLabel="Sub-ms Producer Write"
          isShadowMode={isShadowMonarchMode}
        />

        {/* 5. Ingress Throughput */}
        <CircularGauge
          label="Throughput"
          value={currentTelemetry.ingressEps}
          unit="eps"
          min={0}
          max={50}
          thresholds={{ yellow: 35, red: 48 }}
          icon={<Database className="w-3.5 h-3.5" />}
          sparklineData={epsHistory}
          color={isShadowMonarchMode ? '#c084fc' : '#33B5E5'}
          subLabel="Events / Second"
          isShadowMode={isShadowMonarchMode}
        />

        {/* 6. Epoch Lease & Dragon Quorum */}
        <CircularGauge
          label="Epoch Lease"
          value={nodes.EU_WEST.leaseExpiresInSec}
          unit="sec"
          min={0}
          max={10}
          thresholds={{ yellow: 3, red: 1 }}
          reverseThresholds={true}
          icon={<ShieldCheck className="w-3.5 h-3.5" />}
          sparklineData={quorumLeaseHistory}
          decimals={1}
          subLabel={
            nodes.EU_WEST.leaseExpiresInSec <= 1
              ? 'Renewing Quorum...'
              : nodes.EU_WEST.leaseExpiresInSec <= 3
              ? 'Heartbeat Pending'
              : 'CAS Fencing Active'
          }
          isShadowMode={isShadowMonarchMode}
        />
      </div>
    </div>
  );
};
