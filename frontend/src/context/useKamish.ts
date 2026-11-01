import { createContext, useContext } from "react";
import type { RegionId, ResidencyPolicy, AuditRecord, StreamEvent, RegionNodeState,
  LinkState, TelemetrySample, ScenarioState, ScenarioId } from "../types";
import type { GatewayUser, GatewayStatus, AuthorityState } from "../utils/gatewayClient";

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
  // Phase 5 Gateway Connectivity & Session
  isLiveMode: boolean;
  toggleMode: () => Promise<void>;
  gatewayStatus: GatewayStatus | null;
  gatewayAuthority: AuthorityState | null;
  currentUser: GatewayUser | null;
  gatewayError: string | null;
  isSessionExpired: boolean;
  loginToGateway: (u: string, p: string) => Promise<void>;
  logoutFromGateway: () => Promise<void>;
  refreshLiveTelemetry: () => Promise<void>;
}

export const KamishContext = createContext<KamishContextType | null>(null);
export function useKamish() {
  const context = useContext(KamishContext);
  if (!context) throw new Error("useKamish must be used within a KamishProvider");
  return context;
}
