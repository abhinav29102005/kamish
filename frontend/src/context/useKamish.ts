import { createContext, useContext } from 'react';
import type { RegionId, ResidencyPolicy, AuditRecord, StreamEvent, RegionNodeState,
  LinkState, TelemetrySample, ScenarioState, ScenarioId } from '../types';

export interface KamishContextType {
  nodes: Record<RegionId, RegionNodeState>;
  links: LinkState[];
  telemetryHistory: TelemetrySample[];
  currentTelemetry: TelemetrySample;
  auditLedger: AuditRecord[];
  auditError: string | null;
  recentEvents: StreamEvent[];
  scenarioState: ScenarioState;
  wanDelayMs: number;
  setWanDelayMs: (ms: number) => void;
  isPartitioned: boolean;
  setIsPartitioned: (value: boolean) => void;
  adaptiveRoutingActive: boolean;
  setAdaptiveRoutingActive: (value: boolean) => void;
  trafficRateEps: number;
  setTrafficRateEps: (value: number) => void;
  isShadowMonarchMode: boolean;
  setIsShadowMonarchMode: (value: boolean) => void;
  runScenario: (id: ScenarioId) => void;
  resetCluster: () => void;
  produceCustomEvent: (topic: string, key: string, policy: ResidencyPolicy, target?: RegionId) => Promise<boolean>;
  verifyAuditLedgerIntegrity: () => Promise<{ valid: boolean; count: number; error?: string }>;
}

export const KamishContext = createContext<KamishContextType | null>(null);
export function useKamish() {
  const context = useContext(KamishContext);
  if (!context) throw new Error('useKamish must be used within a KamishProvider');
  return context;
}
