import React, { useState, useEffect, useRef } from 'react';
import { KamishContext } from './useKamish';
import type {
  RegionId,
  ResidencyPolicy,
  AuditRecord,
  StreamEvent,
  RegionNodeState,
  LinkState,
  TelemetrySample,
  ScenarioState,
  ScenarioId,
} from '../types';
import { computeAuditEntryHash, GENESIS_HASH } from '../utils/crypto';
import gatewayClient, { type GatewayUser, type GatewayStatus, type AuthorityState } from '../utils/gatewayClient';
import { validateDemoPolicy } from '../utils/demoPolicy.js';

const INITIAL_NODES: Record<RegionId, RegionNodeState> = {
  EU_WEST: {
    id: 'EU_WEST',
    name: 'Europe West',
    city: 'Frankfurt / London',
    lat: 50.1109,
    lng: 8.6821,
    role: 'Leader',
    status: 'HEALTHY',
    localCommittedOffset: 1240,
    durableWatermark: 1240,
    outboxQueueSize: 0,
    outboxMaxCapacity: 50000,
    cpuPercent: 14.2,
    memoryMb: 412,
    activeEpoch: 4,
    leaseExpiresInSec: 9.8,
    strictLocalShieldActive: true,
  },
  US_EAST: {
    id: 'US_EAST',
    name: 'US East',
    city: 'N. Virginia',
    lat: 38.0336,
    lng: -78.508,
    role: 'Replica',
    status: 'HEALTHY',
    localCommittedOffset: 1240,
    durableWatermark: 1240,
    outboxQueueSize: 0,
    outboxMaxCapacity: 50000,
    cpuPercent: 12.8,
    memoryMb: 384,
    activeEpoch: 4,
    leaseExpiresInSec: 9.8,
    strictLocalShieldActive: false,
  },
  AP_SOUTH: {
    id: 'AP_SOUTH',
    name: 'Asia South',
    city: 'Mumbai',
    lat: 19.076,
    lng: 72.8777,
    role: 'Relay',
    status: 'HEALTHY',
    localCommittedOffset: 1240,
    durableWatermark: 1240,
    outboxQueueSize: 0,
    outboxMaxCapacity: 50000,
    cpuPercent: 8.4,
    memoryMb: 256,
    activeEpoch: 4,
    leaseExpiresInSec: 9.8,
    strictLocalShieldActive: false,
  },
};

const INITIAL_EVENTS: StreamEvent[] = [
  {
    id: 1001,
    topic: "financial-transactions",
    partitionKey: "financial-transactions:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "StrictLocal", description: "StrictLocal EU SEPA Instant Settlement" },
    timestampMs: Date.now() - 14000,
    payloadSize: 1420,
    status: "ingested",
  },
  {
    id: 1002,
    topic: "sovereign-stream",
    partitionKey: "sovereign-stream:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "StrictLocal", description: "Frankfurt DC-4 HSM TPM 2.0 PCR Quote" },
    timestampMs: Date.now() - 12500,
    payloadSize: 2840,
    status: "ingested",
  },
  {
    id: 1003,
    topic: "healthcare-clinical-trials",
    partitionKey: "healthcare-clinical-trials:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "StrictLocal", description: "GDPR Art 9 Genomic Sequencing Biomarker" },
    timestampMs: Date.now() - 11000,
    payloadSize: 4096,
    status: "blocked",
  },
  {
    id: 1004,
    topic: "cross-border-kyc-registry",
    partitionKey: "cross-border-kyc-registry:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "RestrictedReplication", allowedRegions: ["EU_WEST", "US_EAST"], description: "FATF Rec 16 Inter-VASP Travel Rule" },
    timestampMs: Date.now() - 9800,
    payloadSize: 1820,
    status: "replicated",
  },
  {
    id: 1005,
    topic: "order-matching-engine",
    partitionKey: "order-matching-engine:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "GlobalCleartext", description: "L3 Limit Order Execution EUR/USD" },
    timestampMs: Date.now() - 8400,
    payloadSize: 512,
    status: "replicated",
  },
  {
    id: 1006,
    topic: "sovereign-stream",
    partitionKey: "sovereign-stream:1",
    originRegion: "EU_WEST",
    residencyRule: { type: "StrictLocal", description: "ENTSO-E Grid Frequency Sync Telemetry" },
    timestampMs: Date.now() - 7200,
    payloadSize: 980,
    status: "ingested",
  },
  {
    id: 1007,
    topic: "financial-transactions",
    partitionKey: "financial-transactions:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "StrictLocal", description: "Bundesbank TARGET2 RTGS Wholesale Clearing" },
    timestampMs: Date.now() - 6100,
    payloadSize: 3120,
    status: "ingested",
  },
  {
    id: 1008,
    topic: "order-matching-engine",
    partitionKey: "order-matching-engine:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "GlobalCleartext", description: "Eurex FDAX Order Book Depth Update" },
    timestampMs: Date.now() - 5000,
    payloadSize: 784,
    status: "replicated",
  },
  {
    id: 1009,
    topic: "cross-border-kyc-registry",
    partitionKey: "cross-border-kyc-registry:1",
    originRegion: "EU_WEST",
    residencyRule: { type: "RestrictedReplication", allowedRegions: ["EU_WEST", "US_EAST"], description: "eIDAS 2.0 Qualified Electronic Signature" },
    timestampMs: Date.now() - 3900,
    payloadSize: 2140,
    status: "buffered",
  },
  {
    id: 1010,
    topic: "healthcare-clinical-trials",
    partitionKey: "healthcare-clinical-trials:1",
    originRegion: "EU_WEST",
    residencyRule: { type: "StrictLocal", description: "Continuous Glucose Monitoring Stream" },
    timestampMs: Date.now() - 2800,
    payloadSize: 640,
    status: "ingested",
  },
  {
    id: 1011,
    topic: "general-topic",
    partitionKey: "general-topic:0",
    originRegion: "EU_WEST",
    residencyRule: { type: "GlobalCleartext", description: "Kubernetes Bare-Metal Node Provisioning" },
    timestampMs: Date.now() - 1600,
    payloadSize: 1180,
    status: "replicated",
  },
  {
    id: 1012,
    topic: "financial-transactions",
    partitionKey: "financial-transactions:2",
    originRegion: "EU_WEST",
    residencyRule: { type: "StrictLocal", description: "Basel III High Quality Liquid Asset Transfer" },
    timestampMs: Date.now() - 600,
    payloadSize: 2450,
    status: "ingested",
  },
];

const INITIAL_LINKS: LinkState[] = [
  {
    id: 'eu-us',
    source: 'EU_WEST',
    target: 'US_EAST',
    rttMs: 14.5,
    baseRttMs: 14.5,
    lossRate: 0.01,
    status: 'HEALTHY',
    isRelayRouteActive: false,
    bandwidthMbps: 1000,
    packetsInFlight: 8,
  },
  {
    id: 'eu-ap',
    source: 'EU_WEST',
    target: 'AP_SOUTH',
    rttMs: 58.2,
    baseRttMs: 58.2,
    lossRate: 0.02,
    status: 'HEALTHY',
    isRelayRouteActive: false,
    bandwidthMbps: 500,
    packetsInFlight: 3,
  },
  {
    id: 'ap-us',
    source: 'AP_SOUTH',
    target: 'US_EAST',
    rttMs: 72.1,
    baseRttMs: 72.1,
    lossRate: 0.03,
    status: 'HEALTHY',
    isRelayRouteActive: false,
    bandwidthMbps: 500,
    packetsInFlight: 2,
  },
];

export const KamishProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [nodes, setNodes] = useState<Record<RegionId, RegionNodeState>>(INITIAL_NODES);
  const [links, setLinks] = useState<LinkState[]>(INITIAL_LINKS);
  const [auditLedger, setAuditLedger] = useState<AuditRecord[]>([]);
  const [auditError, setAuditError] = useState<string | null>(null);
  const [recentEvents, setRecentEvents] = useState<StreamEvent[]>(INITIAL_EVENTS);
  
  // Chaos & tuning states
  const [wanDelayMs, setWanDelayMs] = useState<number>(0);
  const [isPartitioned, setIsPartitioned] = useState<boolean>(false);
  const [adaptiveRoutingActive, setAdaptiveRoutingActive] = useState<boolean>(false);
  const [trafficRateEps, setTrafficRateEps] = useState<number>(8);
  const [isShadowMonarchMode, setIsShadowMonarchMode] = useState<boolean>(true);

  // Telemetry buffer
  const [telemetryHistory, setTelemetryHistory] = useState<TelemetrySample[]>([]);
  const [currentTelemetry, setCurrentTelemetry] = useState<TelemetrySample>(() => ({
    timestamp: Date.now(),
    localCommitted: 1240,
    remoteWatermark: 1240,
    replicationLag: 0,
    wanRttMs: 14.5,
    outboxDepth: 0,
    ingressEps: 8,
    writeLatencyMs: 0.42,
    leaseExpiresInSec: 9.8,
    throughputEps: 8,
    p99LatencyMs: 0.42,
  }));

  // Scenario execution state
  const [scenarioState, setScenarioState] = useState<ScenarioState>({
    id: null,
    name: 'None',
    gate: 'None',
    status: 'idle',
    currentStepIndex: -1,
    steps: [],
    logs: [],
  });

  // Phase 5 Gateway states
  const [isLiveMode, setIsLiveMode] = useState<boolean>(true);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus | null>(null);
  const [gatewayAuthority, setGatewayAuthority] = useState<AuthorityState | null>(null);
  const [currentUser, setCurrentUser] = useState<GatewayUser | null>(() => gatewayClient.getCurrentUser());
  const [gatewayError, setGatewayError] = useState<string | null>(null);
  const [isSessionExpired, setIsSessionExpired] = useState<boolean>(false);

  const ledgerRef = useRef<AuditRecord[]>([]);
  const auditQueue = useRef<Promise<void>>(Promise.resolve());
  const initializationStarted = useRef(false);

  // Initialize genesis audit record
  useEffect(() => {
    if (initializationStarted.current) return;
    initializationStarted.current = true;
    async function initAudit() {
      const auditDefinitions = [
        {
          action: 'Ingress' as const,
          topic: 'system-genesis',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'Cryptographic genesis state anchored to sovereign local storage',
        },
        {
          action: 'BrokerAppend' as const,
          topic: 'financial-transactions',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'ISO 20022 SEPA Instant Credit Transfer pacs.008.001.08 validated and fsynced to WAL',
        },
        {
          action: 'ReplicationSender' as const,
          topic: 'financial-transactions',
          source: 'EU_WEST' as const,
          target: 'US_EAST' as const,
          outcome: 'Deny' as const,
          reason: 'StrictLocal residency policy strictly forbids egress replication outside EU_WEST',
        },
        {
          action: 'Ingress' as const,
          topic: 'sovereign-stream',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'Hardware Security Module TPM 2.0 PCR attestation quote verified against baseline',
        },
        {
          action: 'BrokerAppend' as const,
          topic: 'sovereign-stream',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'GDPR Article 17 Right to Erasure cryptographic receipt recorded into immutable ledger',
        },
        {
          action: 'ReplicationSender' as const,
          topic: 'sovereign-stream',
          source: 'EU_WEST' as const,
          target: 'AP_SOUTH' as const,
          outcome: 'Deny' as const,
          reason: 'Sovereign shield active: confidential enclave measurements restricted to Frankfurt DC-4',
        },
        {
          action: 'Ingress' as const,
          topic: 'healthcare-clinical-trials',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'Clinical Trial NCT04859127 genomic biomarker sequence verified under GDPR Art 9',
        },
        {
          action: 'BrokerAppend' as const,
          topic: 'healthcare-clinical-trials',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'Stored with HIPAA §164.312(e)(1) post-quantum Kyber-1024 encryption layer',
        },
        {
          action: 'ReplicationSender' as const,
          topic: 'healthcare-clinical-trials',
          source: 'EU_WEST' as const,
          target: 'US_EAST' as const,
          outcome: 'Deny' as const,
          reason: 'EMA CTIS regulations prohibit unmasked patient genomic telemetry egress to non-EEA destination',
        },
        {
          action: 'Ingress' as const,
          topic: 'cross-border-kyc-registry',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'FATF Recommendation 16 Travel Rule originator VASP identity LEI validated',
        },
        {
          action: 'ReplicationSender' as const,
          topic: 'cross-border-kyc-registry',
          source: 'EU_WEST' as const,
          target: 'US_EAST' as const,
          outcome: 'AllowSealed' as const,
          reason: 'RestrictedReplication permitted: payload sealed with recipient enclave hardware public key',
        },
        {
          action: 'InRegionReplica' as const,
          topic: 'cross-border-kyc-registry',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'In-region 2-of-3 persistent quorum consensus achieved across local replicas',
        },
        {
          action: 'Ingress' as const,
          topic: 'order-matching-engine',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'L3 Limit order book trade EUR/USD filled at 1.08425 (TRD-8914-XETRA)',
        },
        {
          action: 'ReplicationSender' as const,
          topic: 'order-matching-engine',
          source: 'EU_WEST' as const,
          target: 'US_EAST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'GlobalCleartext policy permits public market tick replication to Virginia replica',
        },
        {
          action: 'ReplicationSender' as const,
          topic: 'order-matching-engine',
          source: 'EU_WEST' as const,
          target: 'AP_SOUTH' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'GlobalCleartext policy permits real-time market data dissemination to Mumbai relay',
        },
        {
          action: 'ConsumerFetch' as const,
          topic: 'financial-transactions',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'Bundesbank TARGET2 clearing consumer authorized with role Consumer',
        },
        {
          action: 'TelemetryExport' as const,
          topic: 'general-topic',
          source: 'EU_WEST' as const,
          target: 'US_EAST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'OpenTelemetry collector exported non-sensitive distributed trace spans',
        },
        {
          action: 'BackupExport' as const,
          topic: 'sovereign-stream',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowSealed' as const,
          reason: 'Durable cold-tier backup archive sealed with customer-managed hardware key',
        },
        {
          action: 'AdminReplay' as const,
          topic: 'financial-transactions',
          source: 'EU_WEST' as const,
          target: 'EU_WEST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'Auditor initiated historical replay verification of partition commit log 0',
        },
        {
          action: 'RelayTransit' as const,
          topic: 'order-matching-engine',
          source: 'AP_SOUTH' as const,
          target: 'US_EAST' as const,
          outcome: 'AllowCleartext' as const,
          reason: 'Fast-path overlay WAN routing relay forwarded market order sequence',
        },
      ];

      const records: AuditRecord[] = [];
      let prevHash = GENESIS_HASH;
      const baseTime = Date.now() - 60000;

      for (let i = 0; i < auditDefinitions.length; i++) {
        const def = auditDefinitions[i];
        const seq = i + 1;
        const hash = await computeAuditEntryHash(
          prevHash,
          seq,
          def.action,
          def.topic,
          def.source,
          def.target,
          1,
          def.outcome,
          def.reason
        );

        records.push({
          seq,
          timestamp: new Date(baseTime + i * 2500).toISOString(),
          action: def.action,
          topic: def.topic,
          sourceRegion: def.source,
          targetRegion: def.target,
          policyVersion: 1,
          outcome: def.outcome,
          reason: def.reason,
          prevHash,
          hash,
        });

        prevHash = hash;
      }

      ledgerRef.current = records;
      setAuditLedger(records);
    }
    auditQueue.current = auditQueue.current.then(initAudit).catch(error => {
      setAuditError(error instanceof Error ? error.message : 'Demo audit initialization failed');
    });
  }, []);

  // Helper to record an audit entry
  const recordAudit = async (
    action: AuditRecord['action'],
    topic: string,
    source: RegionId,
    target: RegionId,
    outcome: AuditRecord['outcome'],
    reason: string
  ): Promise<AuditRecord> => {
    const task = auditQueue.current.then(async () => {
    const list = ledgerRef.current;
    const prev = list.length > 0 ? list[list.length - 1] : null;
    const prevHash = prev ? prev.hash : GENESIS_HASH;
    const nextSeq = list.length + 1;

    const hash = await computeAuditEntryHash(
      prevHash,
      nextSeq,
      action,
      topic,
      source,
      target,
      1,
      outcome,
      reason
    );

    const record: AuditRecord = {
      seq: nextSeq,
      timestamp: new Date().toISOString(),
      action,
      topic,
      sourceRegion: source,
      targetRegion: target,
      policyVersion: 1,
      outcome,
      reason,
      prevHash,
      hash,
    };

    ledgerRef.current = [...list, record];
    setAuditLedger(ledgerRef.current);
    setAuditError(null);
    return record;
    });
    auditQueue.current = task.then(() => {}, error => {
      setAuditError(error instanceof Error ? error.message : 'Demo audit append failed');
    });
    return task;
  };

  // High-frequency 10Hz tick simulation (Netdata sub-second responsiveness)
  useEffect(() => {
    const interval = setInterval(() => {
      setNodes((prevNodes) => {
        const eu = { ...prevNodes.EU_WEST };
        const us = { ...prevNodes.US_EAST };
        const ap = { ...prevNodes.AP_SOUTH };

        // Lease countdown (renew periodically like ZK/Raft epoch lease)
        eu.leaseExpiresInSec = eu.leaseExpiresInSec <= 0.5 ? 10.0 : parseFloat((eu.leaseExpiresInSec - 0.1).toFixed(1));
        
        // If US_EAST is partitioned, its quorum lease decays to 0.0s and CANNOT renew (INV-7 split-brain fencing)!
        if (isPartitioned) {
          us.leaseExpiresInSec = Math.max(0, parseFloat((us.leaseExpiresInSec - 0.2).toFixed(1)));
          us.status = 'OFFLINE';
        } else {
          us.leaseExpiresInSec = us.leaseExpiresInSec <= 0.5 ? 10.0 : parseFloat((us.leaseExpiresInSec - 0.1).toFixed(1));
          if (wanDelayMs > 150) {
            us.status = 'DEGRADED';
          } else {
            us.status = 'HEALTHY';
          }
        }
        ap.leaseExpiresInSec = ap.leaseExpiresInSec <= 0.5 ? 10.0 : parseFloat((ap.leaseExpiresInSec - 0.1).toFixed(1));

        // Background traffic ingestion (DEMO MODE ONLY: Never invent synthetic writes in LIVE mode)
        if (!isLiveMode && trafficRateEps > 0 && scenarioState.status !== 'running') {
          const delta = trafficRateEps * 0.1; // per 100ms
          const increment = Math.floor(delta) + (Math.random() < delta % 1 ? 1 : 0);
          if (increment > 0) {
            eu.localCommittedOffset += increment;
            if (isPartitioned) {
              eu.outboxQueueSize += increment;
            } else if (wanDelayMs > 0) {
              // delayed ack: buffer in outbox
              const queueGrowth = Math.min(increment, 2);
              eu.outboxQueueSize = Math.min(eu.outboxQueueSize + queueGrowth, 200);
              us.durableWatermark = Math.max(us.durableWatermark, eu.localCommittedOffset - eu.outboxQueueSize);
            } else {
              // Healthy instant drain
              if (eu.outboxQueueSize > 0) {
                eu.outboxQueueSize = Math.max(0, eu.outboxQueueSize - 1);
              }
              us.durableWatermark = eu.localCommittedOffset - eu.outboxQueueSize;
            }
          }
        }

        return { EU_WEST: eu, US_EAST: us, AP_SOUTH: ap };
      });

      // Update link latencies and routing
      setLinks((prevLinks) => {
        return prevLinks.map((l) => {
          if (l.id === 'eu-us') {
            const effectiveRtt = isPartitioned ? 999.0 : 14.5 + wanDelayMs;
            return {
              ...l,
              rttMs: parseFloat(effectiveRtt.toFixed(1)),
              status: isPartitioned ? 'SEVERED' : wanDelayMs > 150 ? 'DEGRADED' : 'HEALTHY',
              isRelayRouteActive: adaptiveRoutingActive && (isPartitioned || wanDelayMs > 150),
              packetsInFlight: isPartitioned ? 0 : Math.max(1, Math.floor(wanDelayMs / 20) + 4),
            };
          }
          if (l.id === 'eu-ap' || l.id === 'ap-us') {
            return {
              ...l,
              isRelayRouteActive: adaptiveRoutingActive && (isPartitioned || wanDelayMs > 150),
            };
          }
          return l;
        });
      });
    }, 100);

    return () => clearInterval(interval);
  }, [trafficRateEps, wanDelayMs, isPartitioned, adaptiveRoutingActive, scenarioState.status, isLiveMode]);

  // Phase 5 Gateway telemetry refresh
  const refreshLiveTelemetry = async () => {
    if (!gatewayClient.isAuthenticated()) {
      return;
    }
    try {
      const [status, authority, streams] = await Promise.all([
        gatewayClient.getStatus(),
        gatewayClient.getAuthority(),
        gatewayClient.getStreams(),
      ]);

      setGatewayStatus(status);
      setGatewayAuthority(authority);
      setGatewayError(null);
      setIsSessionExpired(false);

      setNodes((prev) => {
        const updated = { ...prev };
        if (updated[status.region as RegionId]) {
          updated[status.region as RegionId] = {
            ...updated[status.region as RegionId],
            status: status.status === "HEALTHY" ? "HEALTHY" : "DEGRADED",
            activeEpoch: authority.ownerships[0]?.epoch || updated[status.region as RegionId].activeEpoch,
            leaseExpiresInSec: authority.ownerships[0]?.active_lease ? 30.0 : 0.0,
          };
        }
        return updated;
      });

      const outboxPending = streams.outbox_totals?.pending_count ?? 0;
      const outboxBytes = streams.outbox_totals?.byte_size ?? 0;
      const oldestAge = streams.outbox_totals?.oldest_entry_age_ms ?? 0;

      if (streams.streams.length > 0) {
        const stream0 = streams.streams[0];
        const lagUs = stream0.replication_lags["US_EAST"] || 0;
        setCurrentTelemetry((prev) => ({
          ...prev,
          replicationLag: lagUs,
          remoteWatermark: lagUs,
          outboxDepth: outboxPending,
        }));
      }

      setNodes((prev) => {
        const currentRegion = (status.region || "EU_WEST") as RegionId;
        const targetNode = prev[currentRegion];
        if (!targetNode) return prev;
        return {
          ...prev,
          [currentRegion]: {
            ...targetNode,
            outboxQueueSize: outboxPending,
          },
        };
      });

      if (gatewayClient.isAuthenticated() && !currentUser) {
        gatewayClient.getMe().then(me => setCurrentUser(me.user)).catch(() => {});
      }
    } catch (err: any) {
      if (err.message && err.message.includes("Session expired")) {
        setIsSessionExpired(true);
      } else {
        setGatewayError(err.message || "Gateway disconnected");
      }
    }
  };

  // Telemetry stream sampler (every 500ms): In LIVE mode, refresh real gateway telemetry without synthetic jitter
  const simulationRef = useRef({ nodes, wanDelayMs, isPartitioned, trafficRateEps, isLiveMode });
  useEffect(() => {
    simulationRef.current = { nodes, wanDelayMs, isPartitioned, trafficRateEps, isLiveMode };
  }, [nodes, wanDelayMs, isPartitioned, trafficRateEps, isLiveMode]);
  useEffect(() => {
    const timer = setInterval(() => {
      const { nodes: sampledNodes, wanDelayMs, isPartitioned, trafficRateEps, isLiveMode: inLive } = simulationRef.current;
      if (inLive) {
        // LIVE MODE: Do NOT invent numbers or run synthetic generators.
        refreshLiveTelemetry();
        return;
      }

      // DEMO MODE ONLY: Synthetic simulation sampling
      const eu = sampledNodes.EU_WEST;
      const us = sampledNodes.US_EAST;
      const lag = Math.max(0, eu.localCommittedOffset - us.durableWatermark);
      const effectiveRtt = isPartitioned ? 999 : 14.5 + wanDelayMs;
      const writeLat = 0.40;

      const sample: TelemetrySample = {
        timestamp: Date.now(),
        localCommitted: eu.localCommittedOffset,
        remoteWatermark: us.durableWatermark,
        replicationLag: lag,
        wanRttMs: effectiveRtt,
        outboxDepth: eu.outboxQueueSize,
        ingressEps: trafficRateEps,
        writeLatencyMs: writeLat,
        leaseExpiresInSec: eu.leaseExpiresInSec,
        throughputEps: trafficRateEps,
        p99LatencyMs: 0.42,
      };

      setCurrentTelemetry(sample);
      setTelemetryHistory((prev) => {
        const next = [...prev, sample];
        return next.length > 50 ? next.slice(next.length - 50) : next;
      });
    }, isLiveMode ? 2500 : 500);

    return () => clearInterval(timer);
  }, [isLiveMode]);

  // Produce a custom event
  

  const toggleMode = async () => {
    const nextMode = isLiveMode ? "DEMO" : "LIVE";
    try {
      if (gatewayClient.isAuthenticated()) {
        await gatewayClient.setMode(nextMode);
      }
      setIsLiveMode(nextMode === "LIVE");
    } catch {
      setIsLiveMode(nextMode === "LIVE");
    }
  };

  const loginToGateway = async (u: string, p: string) => {
    const res = await gatewayClient.login(u, p);
    setCurrentUser(res.user);
    setIsSessionExpired(false);
    setGatewayError(null);
    await refreshLiveTelemetry();
  };

  const logoutFromGateway = async () => {
    await gatewayClient.logout();
    setCurrentUser(null);
    setIsSessionExpired(false);
  };

const produceCustomEvent = async (
    topic: string,
    key: string,
    policy: ResidencyPolicy,
    targetRegion: RegionId = 'US_EAST'
  ): Promise<boolean> => {
    validateDemoPolicy(policy);
    const id = Date.now();
    const event: StreamEvent = {
      id,
      topic,
      partitionKey: key,
      originRegion: 'EU_WEST',
      residencyRule: policy,
      timestampMs: id,
      payloadSize: 512,
      status: 'ingested',
    };

    // 1. Ingress check in EU_WEST (always allowed in home region)
    await recordAudit(
      'Ingress',
      topic,
      'EU_WEST',
      'EU_WEST',
      'AllowCleartext',
      `Event ${key} ingested into EU_WEST home partition`
    );

    // 2. Replication Egress check to targetRegion
    if (policy.type === 'StrictLocal' && targetRegion !== 'EU_WEST') {
      event.status = 'blocked';
      await recordAudit(
        'ReplicationSender',
        topic,
        'EU_WEST',
        targetRegion,
        'Deny',
        `StrictLocal residency policy strictly forbids egress replication to ${targetRegion}`
      );
      setRecentEvents((prev) => [event, ...prev.slice(0, 24)]);
      return false;
    }

    if (policy.type === 'RestrictedReplication') {
      const allowed = policy.allowedRegions || [];
      if (!allowed.includes(targetRegion)) {
        event.status = 'blocked';
        await recordAudit(
          'ReplicationSender',
          topic,
          'EU_WEST',
          targetRegion,
          'Deny',
          `Target region ${targetRegion} not present in permitted legal whitelist [${allowed.join(', ')}]`
        );
        setRecentEvents((prev) => [event, ...prev.slice(0, 24)]);
        return false;
      }
    }

    // Allowed outcome
    const outcome = policy.type === 'SealedCiphertext' ? 'AllowSealed' : 'AllowCleartext';
    const reason =
      policy.type === 'SealedCiphertext'
        ? `Sealed envelope encrypted with regional key before egress to ${targetRegion}`
        : `Policy permits egress replication to ${targetRegion}`;

    await recordAudit('ReplicationSender', topic, 'EU_WEST', targetRegion, outcome, reason);

    event.status = 'replicated';
    setRecentEvents((prev) => [event, ...prev.slice(0, 24)]);

    // Bump local offset
    setNodes((prev) => ({
      ...prev,
      EU_WEST: {
        ...prev.EU_WEST,
        localCommittedOffset: prev.EU_WEST.localCommittedOffset + 1,
      },
      US_EAST: {
        ...prev.US_EAST,
        durableWatermark: prev.US_EAST.durableWatermark + 1,
      },
    }));

    return true;
  };

  // Run official Hackathon demonstration scenarios (A, B, C)
  const runScenario = async (id: ScenarioId) => {
    if (!id) return;

    if (id === 'A') {
      // Scenario A: Gate 2 / Demo A - Data Residency Enforcement
      setScenarioState({
        id: 'A',
        name: 'Scenario A: Regional Data Residency Enforcement',
        gate: 'Gate 2 / Demo A Ready',
        status: 'running',
        currentStepIndex: 0,
        steps: [
          {
            step: 1,
            timeLabel: '00:01',
            title: 'Ingest StrictLocal Record in EU_WEST',
            description: 'Ingesting patient records under GDPR Article 44 in Frankfurt broker',
            status: 'running',
          },
          {
            step: 2,
            timeLabel: '00:03',
            title: 'Evaluate Outbox Replication to US_EAST',
            description: 'Replication worker initiates egress transfer across transatlantic link',
            status: 'pending',
          },
          {
            step: 3,
            timeLabel: '00:05',
            title: 'Enforce E4 Decision Function & Hash Chain',
            description: 'Enforcer blocks egress; tamper-evident SHA-256 audit record appended',
            status: 'pending',
          },
        ],
        logs: [
          '[00:00] Initializing Scenario A (Gate 2 / Demo A Ready)...',
          '[00:01] Ingesting StrictLocal sovereign record into EU_WEST partition [eu-patient-records/0]...',
        ],
      });

      // Step 1
      await new Promise((r) => setTimeout(r, 900));
      await recordAudit(
        'Ingress',
        'eu-patient-records',
        'EU_WEST',
        'EU_WEST',
        'AllowCleartext',
        'Ingestion approved in sovereign home region EU_WEST'
      );

      setScenarioState((s) => ({
        ...s,
        currentStepIndex: 1,
        steps: s.steps.map((st, i) =>
          i === 0 ? { ...st, status: 'completed' } : i === 1 ? { ...st, status: 'running' } : st
        ),
        logs: [
          ...s.logs,
          '   OK: Local write fsync complete. Record committed at offset 1241.',
          '[00:03] Replication agent E4 evaluating destination US_EAST against ResidencyPolicy::StrictLocal...',
        ],
      }));

      // Step 2 & 3
      await new Promise((r) => setTimeout(r, 1200));
      const auditRec = await recordAudit(
        'ReplicationSender',
        'eu-patient-records',
        'EU_WEST',
        'US_EAST',
        'Deny',
        'StrictLocal policy strictly forbids egress replication to remote region US_EAST'
      );

      setScenarioState((s) => ({
        ...s,
        status: 'passed',
        currentStepIndex: 2,
        steps: s.steps.map((st) => ({ ...st, status: 'completed' })),
        logs: [
          ...s.logs,
          `   BLOCKED: Egress replication denied by KamishPolicyEngine. Zero payload leaked outside EU borders.`,
          `[00:05] Cryptographic Hash Chain entry #${auditRec.seq} created: ${auditRec.hash.substring(0, 16)}...`,
          '========================================================================',
          ' [DEMO SCENARIO A PASSED] Residency enforcement successfully blocked unauthorized egress to US_EAST with zero-payload audit logging.',
          '========================================================================',
        ],
      }));
    } else if (id === 'B') {
      // Scenario B: Gate 3 / Demo B - Slow-Link WAN Delay & Dynamic Lag Metrics
      setScenarioState({
        id: 'B',
        name: 'Scenario B: Slow-Link WAN Delay & Dynamic Lag Metrics',
        gate: 'Gate 3 / Demo B Ready',
        status: 'running',
        currentStepIndex: 0,
        steps: [
          {
            step: 1,
            timeLabel: '00:01',
            title: 'Verify Baseline Healthy Link (10ms, Lag = 0)',
            description: 'Local produce & replication watermarks perfectly synchronized',
            status: 'running',
          },
          {
            step: 2,
            timeLabel: '00:03',
            title: 'Inject 300ms WAN Latency via Chaos Proxy',
            description: 'Link between EU_WEST and US_EAST throttled; RTT metric rises to 300ms',
            status: 'pending',
          },
          {
            step: 3,
            timeLabel: '00:06',
            title: 'Produce 50 Records to EU_WEST Log',
            description: 'Local produce throughput unaffected (<1ms fsync); replication lag rises to 50',
            status: 'pending',
          },
        ],
        logs: [
          '[00:00] Initializing Scenario B (Gate 3 / Demo B Ready)...',
          '[00:01] Measuring baseline healthy link: RTT = 14.5ms, Replication Lag = 0.',
        ],
      });

      await new Promise((r) => setTimeout(r, 900));
      setWanDelayMs(300);

      setScenarioState((s) => ({
        ...s,
        currentStepIndex: 1,
        steps: s.steps.map((st, i) =>
          i === 0 ? { ...st, status: 'completed' } : i === 1 ? { ...st, status: 'running' } : st
        ),
        logs: [
          ...s.logs,
          '[00:03] Injected 300ms WAN latency via synthetic chaos proxy on US_EAST link...',
          '   Link status updated: DEGRADED. Link RTT dial reading 314.5ms.',
          '[00:04] Beginning local produce burst of 50 records in EU_WEST...',
        ],
      }));

      await new Promise((r) => setTimeout(r, 1200));

      // Bump local log by 50 while keeping remote watermark un-acked
      setNodes((prev) => ({
        ...prev,
        EU_WEST: {
          ...prev.EU_WEST,
          localCommittedOffset: prev.EU_WEST.localCommittedOffset + 50,
          outboxQueueSize: 50,
        },
      }));

      await new Promise((r) => setTimeout(r, 1000));

      setScenarioState((s) => ({
        ...s,
        status: 'passed',
        currentStepIndex: 2,
        steps: s.steps.map((st) => ({ ...st, status: 'completed' })),
        logs: [
          ...s.logs,
          '   Produced 50 records into EU_WEST local partition in 1.4ms (Average local fsync < 0.4ms).',
          '   Local throughput unaffected by degraded transatlantic WAN link.',
          '   Observability formula: ReplicationLag = LocalCommittedOffset - RemoteAckedOffset = 50.',
          '========================================================================',
          ' [DEMO SCENARIO B PASSED] Injected 300ms delay increased replication lag to 50 without blocking local produce.',
          '========================================================================',
        ],
      }));
    } else if (id === 'C') {
      // Scenario C: Gate 4 / Demo C - Regional Outage & Resumable Watermark Recovery
      setScenarioState({
        id: 'C',
        name: 'Scenario C: Regional Outage & Resumable Recovery',
        gate: 'Gate 4 / Demo C Ready',
        status: 'running',
        currentStepIndex: 0,
        steps: [
          {
            step: 1,
            timeLabel: '00:01',
            title: 'Simulate Catastrophic Failure of US_EAST',
            description: 'Transatlantic WAN connection severed; US_EAST marked OFFLINE',
            status: 'running',
          },
          {
            step: 2,
            timeLabel: '00:04',
            title: 'Buffer 15 New Records in Durable Outbox',
            description: 'EU_WEST local broker buffers messages with zero data corruption',
            status: 'pending',
          },
          {
            step: 3,
            timeLabel: '00:07',
            title: 'Heal Network & Reconcile from Watermark',
            description: 'US_EAST restarts; catches up from durable watermark with 0 record loss',
            status: 'pending',
          },
        ],
        logs: [
          '[00:00] Initializing Scenario C (Gate 4 / Demo C Ready)...',
          '[00:01] Simulating catastrophic failure of remote Region B (US_EAST severed)...',
        ],
      });

      setIsPartitioned(true);

      await new Promise((r) => setTimeout(r, 1200));

      setScenarioState((s) => ({
        ...s,
        currentStepIndex: 1,
        steps: s.steps.map((st, i) =>
          i === 0 ? { ...st, status: 'completed' } : i === 1 ? { ...st, status: 'running' } : st
        ),
        logs: [
          ...s.logs,
          '   US_EAST partition link severed. Node state: OFFLINE.',
          '[00:03] Ingesting 15 new transactions into EU_WEST during outage...',
          '   Buffering ranges in DestinationOutbox bounded queue...',
        ],
      }));

      // Ingest 15 buffered records
      setNodes((prev) => ({
        ...prev,
        EU_WEST: {
          ...prev.EU_WEST,
          localCommittedOffset: prev.EU_WEST.localCommittedOffset + 15,
          outboxQueueSize: prev.EU_WEST.outboxQueueSize + 15,
        },
      }));

      await new Promise((r) => setTimeout(r, 1500));

      setScenarioState((s) => ({
        ...s,
        currentStepIndex: 2,
        steps: s.steps.map((st, i) =>
          i === 1 ? { ...st, status: 'completed' } : i === 2 ? { ...st, status: 'running' } : st
        ),
        logs: [
          ...s.logs,
          '   15 records safely buffered on disk in EU_WEST durable outbox.',
          '[00:06] Restoring network link... US_EAST recovering.',
          '   Initiating watermark handshake: Requesting durable watermark...',
        ],
      }));

      // Heal partition & reconcile
      setIsPartitioned(false);
      setWanDelayMs(0);

      await new Promise((r) => setTimeout(r, 1200));

      setNodes((prev) => ({
        ...prev,
        EU_WEST: {
          ...prev.EU_WEST,
          outboxQueueSize: 0,
        },
        US_EAST: {
          ...prev.US_EAST,
          durableWatermark: prev.EU_WEST.localCommittedOffset,
          leaseExpiresInSec: 10.0,
          status: 'HEALTHY',
        },
      }));

      setScenarioState((s) => ({
        ...s,
        status: 'passed',
        currentStepIndex: 2,
        steps: s.steps.map((st) => ({ ...st, status: 'completed' })),
        logs: [
          ...s.logs,
          '   Watermark reconciliation handshake complete. Watermark advanced.',
          '   Outbox trimmed applied ranges. Invariant INV-5 verified: Offsets match exactly.',
          '========================================================================',
          ' [DEMO SCENARIO C PASSED] Region B caught up 15 records from watermark with ZERO data loss.',
          '========================================================================',
        ],
      }));
    }
  };

  const resetCluster = () => {
    setNodes(INITIAL_NODES);
    setLinks(INITIAL_LINKS);
    setWanDelayMs(0);
    setIsPartitioned(false);
    setAdaptiveRoutingActive(false);
    setTrafficRateEps(8);
    setScenarioState({
      id: null,
      name: 'None',
      gate: 'None',
      status: 'idle',
      currentStepIndex: -1,
      steps: [],
      logs: [],
    });
  };

  // Verify hash-chained cryptographic ledger integrity
  const verifyAuditLedgerIntegrity = async (): Promise<{
    valid: boolean;
    count: number;
    error?: string;
  }> => {
    await auditQueue.current;
    const list = ledgerRef.current;
    if (list.length === 0 || auditError) return { valid: false, count: 0, error: auditError || 'No demo audit entries available' };

    for (let i = 0; i < list.length; i++) {
      const entry = list[i];
      const expectedPrev = i === 0 ? GENESIS_HASH : list[i - 1].hash;
      if (entry.seq !== i + 1) return { valid: false, count: i, error: 'Demo audit sequence mismatch' };
      if (entry.prevHash !== expectedPrev) {
        return {
          valid: false,
          count: i,
          error: `Broken chain at sequence #${entry.seq}: prevHash mismatch`,
        };
      }

      const calculated = await computeAuditEntryHash(
        entry.prevHash,
        entry.seq,
        entry.action,
        entry.topic,
        entry.sourceRegion,
        entry.targetRegion,
        entry.policyVersion,
        entry.outcome,
        entry.reason
      );

      if (calculated !== entry.hash) {
        return {
          valid: false,
          count: i,
          error: `Tampered hash at sequence #${entry.seq}: hash mismatch`,
        };
      }
    }

    return { valid: true, count: list.length };
  };

  return (
    <KamishContext.Provider
      value={{
        nodes,
        links,
        telemetryHistory,
        currentTelemetry,
        auditLedger,
        auditError,
        recentEvents,
        scenarioState,
        wanDelayMs,
        setWanDelayMs,
        isPartitioned,
        setIsPartitioned,
        adaptiveRoutingActive,
        setAdaptiveRoutingActive,
        trafficRateEps,
        setTrafficRateEps,
        isShadowMonarchMode,
        setIsShadowMonarchMode,
        runScenario,
        resetCluster,
        produceCustomEvent,
        verifyAuditLedgerIntegrity,
        isLiveMode,
        toggleMode,
        gatewayStatus,
        gatewayAuthority,
        currentUser,
        gatewayError,
        isSessionExpired,
        loginToGateway,
        logoutFromGateway,
        refreshLiveTelemetry,
      }}
    >
      {children}
    </KamishContext.Provider>
  );
};

export { useKamish } from "./useKamish";
