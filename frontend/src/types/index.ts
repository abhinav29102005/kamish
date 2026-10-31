export type RegionId = 'EU_WEST' | 'US_EAST' | 'AP_SOUTH';

export type ResidencyPolicyType =
  | 'StrictLocal'
  | 'RestrictedReplication'
  | 'GlobalCleartext'
  | 'SealedCiphertext';

export interface ResidencyPolicy {
  type: ResidencyPolicyType;
  allowedRegions?: RegionId[];
  description: string;
}

export type AuditAction =
  | 'Ingress'
  | 'BrokerAppend'
  | 'InRegionReplica'
  | 'ReplicationSender'
  | 'ReplicationReceiver'
  | 'BackupExport'
  | 'ConsumerFetch'
  | 'Promote'
  | 'TelemetryExport'
  | 'AdminReplay'
  | 'RelayTransit';

export type DecisionOutcome = 'AllowCleartext' | 'AllowSealed' | 'Deny';

export interface AuditRecord {
  seq: number;
  timestamp: string;
  action: AuditAction;
  topic: string;
  sourceRegion: RegionId;
  targetRegion: RegionId;
  policyVersion: number;
  outcome: DecisionOutcome;
  reason: string;
  prevHash: string;
  hash: string;
}

export interface StreamEvent {
  id: number;
  topic: string;
  partitionKey: string;
  originRegion: RegionId;
  residencyRule: ResidencyPolicy;
  timestampMs: number;
  payloadSize: number;
  status: 'ingested' | 'replicated' | 'blocked' | 'buffered';
}

export interface RegionNodeState {
  id: RegionId;
  name: string;
  city: string;
  lat: number;
  lng: number;
  role: 'Leader' | 'Replica' | 'Relay';
  status: 'HEALTHY' | 'DEGRADED' | 'OFFLINE';
  localCommittedOffset: number;
  durableWatermark: number;
  outboxQueueSize: number;
  outboxMaxCapacity: number;
  cpuPercent: number;
  memoryMb: number;
  activeEpoch: number;
  leaseExpiresInSec: number;
  strictLocalShieldActive: boolean;
}

export interface LinkState {
  id: string;
  source: RegionId;
  target: RegionId;
  rttMs: number;
  baseRttMs: number;
  lossRate: number;
  status: 'HEALTHY' | 'DEGRADED' | 'SEVERED';
  isRelayRouteActive: boolean;
  bandwidthMbps: number;
  packetsInFlight: number;
}

export interface TelemetrySample {
  timestamp: number;
  localCommitted: number;
  remoteWatermark: number;
  replicationLag: number;
  wanRttMs: number;
  outboxDepth: number;
  ingressEps: number;
  writeLatencyMs: number;
  leaseExpiresInSec: number;
}

export type ScenarioId = 'A' | 'B' | 'C' | null;

export interface ScenarioStep {
  step: number;
  timeLabel: string;
  title: string;
  description: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  terminalLog?: string;
}

export interface ScenarioState {
  id: ScenarioId;
  name: string;
  gate: string;
  status: 'idle' | 'running' | 'passed' | 'failed';
  currentStepIndex: number;
  steps: ScenarioStep[];
  logs: string[];
}
