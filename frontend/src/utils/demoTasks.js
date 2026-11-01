export const DEMO_TASK_KEY = 'kamish.demo.tasks.v1';
export const MAX_DEMO_TASKS = 200;
export const TASK_STATUSES = ['PROTOTYPE', 'PARTIAL', 'DISABLED', 'ACTIVE', 'IN_PROGRESS', 'BLOCKED', 'DONE'];

export const INITIAL_DEMO_TASKS = [
  {
    id: 'TSK-001',
    title: 'Local append log and checksum recovery',
    status: 'DONE',
    owner: 'Storage Engineering (Frankfurt)',
    policy: 'EU_GDPR_STRICT',
    description: 'Segmented persistent commit log with CRC32C frame integrity verification, zero-copy slice iteration, and deterministic crash recovery at segment boundaries.',
    rule: 'INV-1: Immutable Append-Only Commit Log',
  },
  {
    id: 'TSK-002',
    title: 'Idempotent producer session deduplication',
    status: 'DONE',
    owner: 'Protocol Team',
    policy: 'GLOBAL_REPLICATED',
    description: 'Producer sequence tracking with sliding window deduplication cache across active producer IDs and epochs, guaranteeing exactly-once semantics.',
    rule: 'INV-6: Exactly-Once Producer Semantics',
  },
  {
    id: 'TSK-003',
    title: 'Bounded in-memory outbox and log replay',
    status: 'DONE',
    owner: 'Replication Core',
    policy: 'EU_GDPR_STRICT',
    description: 'Multi-peer bounded circular outbox with proactive backpressure threshold at 80% capacity and zero disk-spill memory fencing.',
    rule: 'FR-04: Bounded Asynchronous Outbox Buffer',
  },
  {
    id: 'TSK-004',
    title: 'Ed25519 signed class policy enforcement',
    status: 'DONE',
    owner: 'Security & Enclave Team',
    policy: 'EU_GDPR_STRICT',
    description: 'Cryptographic signature verification on data class definitions and floor version progression, blocking unauthorized policy modifications.',
    rule: 'E1/E2: Continuous Cryptographic Enforcement',
  },
  {
    id: 'TSK-005',
    title: 'Hardware enclave TPM 2.0 PCR attestation',
    status: 'ACTIVE',
    owner: 'Sovereign Enclave Ops',
    policy: 'EU_GDPR_STRICT',
    description: 'Continuous PCR quote verification against trusted hardware enclave baseline digests, validating kernel and BIOS integrity at Frankfurt DC-4.',
    rule: 'SEC-01: Hardware-Rooted Confidential Execution',
  },
  {
    id: 'TSK-006',
    title: 'Policy-filtered egress route selection',
    status: 'DONE',
    owner: 'Network Topology Group',
    policy: 'EU_GDPR_STRICT',
    description: 'Dynamic graph route evaluator that excludes WAN links and peer relay hops that violate destination regional residency policies.',
    rule: 'RES-02: Sovereign Boundary Route Isolation',
  },
  {
    id: 'TSK-007',
    title: 'Distributed CAS partition ownership leases',
    status: 'ACTIVE',
    owner: 'Consensus Engineering',
    policy: 'GLOBAL_REPLICATED',
    description: 'Atomic compare-and-swap epoch fencing with 10-second heartbeat lease renewal and monotonic generation tracking, preventing split-brain writes.',
    rule: 'INV-7: Mutual Exclusive Partition Ownership',
  },
  {
    id: 'TSK-008',
    title: 'Tamper-evident SHA-256 audit hash chain',
    status: 'DONE',
    owner: 'Audit & Compliance',
    policy: 'EU_GDPR_STRICT',
    description: 'Cryptographically linked sequential audit log recording ingress, egress, and routing policy decisions with zero payload data retention.',
    rule: 'AUD-01: Zero-Payload Tamper-Evident Ledger',
  },
  {
    id: 'TSK-009',
    title: 'Real-time canary token storage leak scanner',
    status: 'ACTIVE',
    owner: 'Security Assurance',
    policy: 'EU_GDPR_STRICT',
    description: 'Synthetic honeypot canary injection and automated disk/network scanner that verifies confidential sovereign data never leaves authorized memory boundaries.',
    rule: 'SEC-04: Storage Leak & Canary Integrity Assurance',
  },
  {
    id: 'TSK-010',
    title: 'Zero-downtime leader failover and epoch fencing',
    status: 'ACTIVE',
    owner: 'Reliability Engineering',
    policy: 'GLOBAL_REPLICATED',
    description: 'Automated leader detection and lease expiration handover with fenced fencing tokens, completing replica promotion in under 2.4 seconds.',
    rule: 'HA-03: Sub-Second Lease Decay & Epoch Handover',
  },
  {
    id: 'TSK-011',
    title: 'GDPR Article 17 cryptographic right-to-erasure verifier',
    status: 'IN_PROGRESS',
    owner: 'Privacy Legal & Tech',
    policy: 'EU_GDPR_STRICT',
    description: 'Zero-knowledge blinded proof validation that confirms tombstone records replace user data without invalidating the immutable Merkle chain.',
    rule: 'GDPR-17: Provable Cryptographic Erasure',
  },
  {
    id: 'TSK-012',
    title: 'In-region 2-of-3 persistent quorum replication',
    status: 'DONE',
    owner: 'Storage Engineering',
    policy: 'EU_GDPR_STRICT',
    description: 'Primary-backup local quorum consensus requiring 2 independent disk fsync confirmations before returning produce ACK to client.',
    rule: 'DUR-02: Local Persistence Quorum Guarantee',
  },
  {
    id: 'TSK-013',
    title: 'High-throughput outbox segmented backpressure controller',
    status: 'ACTIVE',
    owner: 'Performance Engineering',
    policy: 'GLOBAL_REPLICATED',
    description: 'Adaptive flow control throttling producers when WAN replication latency exceeds 120ms or outbox depth exceeds 8,000 pending packets.',
    rule: 'NET-03: Dynamic Congestion Avoidance',
  },
  {
    id: 'TSK-014',
    title: 'Post-quantum Kyber-1024 sealed ciphertext replication',
    status: 'IN_PROGRESS',
    owner: 'Cryptography Lab',
    policy: 'EU_GDPR_STRICT',
    description: 'Implementation of CRYSTALS-Kyber-1024 lattice-based key encapsulation for sealed ciphertext egress replication to non-EEA backup targets.',
    rule: 'CRYPTO-03: Post-Quantum Transport Encryption',
  },
  {
    id: 'TSK-015',
    title: 'Cross-border KYC FATF Travel Rule policy gate',
    status: 'ACTIVE',
    owner: 'Financial Regulatory Group',
    policy: 'GLOBAL_REPLICATED',
    description: 'Automated verification of originator and beneficiary legal entity identifiers (LEI) against the FATF Recommendation 16 compliance schema.',
    rule: 'AML-01: Mandatory Inter-VASP Identity Binding',
  },
  {
    id: 'TSK-016',
    title: 'StrictLocal boundary isolation shield',
    status: 'DONE',
    owner: 'Core Kernel Team',
    policy: 'EU_GDPR_STRICT',
    description: 'Hard kernel-level and broker-level boundary filter guaranteeing topics marked StrictLocal drop any cross-region replication attempt.',
    rule: 'SOV-01: Zero-Leak Regional Perimeter Shield',
  },
  {
    id: 'TSK-017',
    title: 'Multi-tenant catalog namespace isolation',
    status: 'DONE',
    owner: 'Platform Architecture',
    policy: 'GLOBAL_REPLICATED',
    description: 'Strict authorization checks preventing cross-tenant topic pollution and ensuring only authorized principals produce to protected data classes.',
    rule: 'AUTH-02: Tenant Namespace Cryptographic Boundary',
  },
  {
    id: 'TSK-018',
    title: 'L3 order book microsecond timestamp precision sync',
    status: 'ACTIVE',
    owner: 'Capital Markets Team',
    policy: 'GLOBAL_REPLICATED',
    description: 'IEEE 1588 Precision Time Protocol (PTP) clock synchronization ensuring MiFID II RTS 25 compliance within 14 nanoseconds of UTC(NPL).',
    rule: 'FIN-02: Sub-Microsecond Clock Traceability',
  },
  {
    id: 'TSK-019',
    title: 'HIPAA §164.312 continuous clinical trial data boundary',
    status: 'ACTIVE',
    owner: 'Healthtech Compliance',
    policy: 'US_DATA_RESIDENCY',
    description: 'Automated pseudonymization and audit logging of clinical trial biomarker telemetry streams, ensuring zero PHI/PII leakage in logs.',
    rule: 'HIPAA-01: End-to-End Medical Data Protection',
  },
  {
    id: 'TSK-020',
    title: 'WAN latency adaptive relay routing',
    status: 'ACTIVE',
    owner: 'Network Topology Group',
    policy: 'GLOBAL_REPLICATED',
    description: 'Sub-second switching to intermediate relay routes (e.g. Frankfurt -> Mumbai -> Virginia) when direct transatlantic fiber experiences degradation.',
    rule: 'WAN-02: Dynamic BGP & Overlay Rerouting',
  },
  {
    id: 'TSK-021',
    title: 'Non-loopback TLS 1.3 mutual authentication barrier',
    status: 'DONE',
    owner: 'Security Assurance',
    policy: 'EU_GDPR_STRICT',
    description: 'Failsafe network listener refusing to bind to public IP addresses without mutual TLS certificates or an explicit `--insecure-dev` flag.',
    rule: 'SEC-03: Mandatory Wire Encryption on Public Interfaces',
  },
  {
    id: 'TSK-022',
    title: 'Split-brain network partition lease decay test',
    status: 'DONE',
    owner: 'Chaos Engineering Team',
    policy: 'GLOBAL_REPLICATED',
    description: 'Automated chaos test injecting network splits and validating that partitioned former leaders fail closed within exactly 10.0 seconds.',
    rule: 'CHAOS-01: Deterministic Partition Fencing',
  },
  {
    id: 'TSK-023',
    title: 'Sub-second Netdata telemetry streaming agent',
    status: 'ACTIVE',
    owner: 'Observability Team',
    policy: 'GLOBAL_REPLICATED',
    description: '10Hz live telemetry socket broadcasting commits, outbox depth, and WAN round-trip time without introducing latency to the broker fast-path.',
    rule: 'OBS-01: Zero-Overhead High-Frequency Telemetry',
  },
  {
    id: 'TSK-024',
    title: 'MiFID II clock synchronization calibration',
    status: 'IN_PROGRESS',
    owner: 'Trading Infrastructure',
    policy: 'GLOBAL_REPLICATED',
    description: 'Continuous drift measurement against atomic rubidium standard clocks with automated alarms when drift exceeds 50 microseconds.',
    rule: 'FIN-03: RTS 25 Drift Monitoring & Auditing',
  },
  {
    id: 'TSK-025',
    title: 'Cryptographic audit checkpoint witness quorum',
    status: 'ACTIVE',
    owner: 'Audit & Compliance',
    policy: 'EU_GDPR_STRICT',
    description: 'Periodic commitment of audit chain tail hashes to external decentralized witness nodes, proving audit ledger completeness.',
    rule: 'AUD-03: External Witness State Anchoring',
  },
  {
    id: 'TSK-026',
    title: 'Basel III intraday liquidity stress test suite',
    status: 'IN_PROGRESS',
    owner: 'Risk Management',
    policy: 'EU_GDPR_STRICT',
    description: 'High-volume synthetic transaction stress harness validating uninterrupted processing of €50M+ interbank wholesale transfers during peak volatility.',
    rule: 'RISK-01: High-Value Settlement Resilience',
  },
  {
    id: 'TSK-027',
    title: 'BSI IT-Grundschutz automated compliance auditor',
    status: 'ACTIVE',
    owner: 'German Sovereign Cloud Desk',
    policy: 'EU_GDPR_STRICT',
    description: 'Automated continuous evaluation of system parameters against BSI-Standard 200-2 IT-Grundschutz module SYS.1.3 (Server Linux).',
    rule: 'BSI-01: Formal Sovereign Security Certification',
  },
  {
    id: 'TSK-028',
    title: 'eIDAS 2.0 Qualified Electronic Signature validator',
    status: 'IN_PROGRESS',
    owner: 'Identity & Access Engineering',
    policy: 'EU_GDPR_STRICT',
    description: 'Integration with EU Qualified Trust Service Providers (QTSP) for cryptographic verification of qualified electronic signatures and seals.',
    rule: 'EIDAS-02: Cross-Border Digital Identity Trust',
  },
  {
    id: 'TSK-029',
    title: 'S3-compatible sovereign object storage export pipeline',
    status: 'ACTIVE',
    owner: 'Data Platform',
    policy: 'EU_GDPR_STRICT',
    description: 'Asynchronous streaming export of cold log segments into client-side encrypted Parquet archives with customer-managed keys (CMK).',
    rule: 'ARCH-01: Customer-Controlled Cold Tier Archival',
  },
  {
    id: 'TSK-030',
    title: 'End-to-end multi-region convergence benchmark',
    status: 'DONE',
    owner: 'Benchmarking & QA',
    policy: 'GLOBAL_REPLICATED',
    description: 'Full 3-region automated test verifying zero-loss, zero-duplicate outbox replication across 100,000 events under simulated WAN delay.',
    rule: 'QA-01: Provable Multi-Region Convergence',
  },
];

function requiredText(value, name, max) {
  if (typeof value !== 'string' || !value.trim() || value.length > max) throw new Error(`Invalid ${name}`);
}

export function validateDemoTask(task) {
  if (!task || typeof task !== 'object') throw new Error('Invalid demo task');
  requiredText(task.id, 'task ID', 100);
  requiredText(task.title, 'title', 256);
  requiredText(task.owner, 'owner', 128);
  if (!TASK_STATUSES.includes(task.status)) throw new Error('Invalid task status');
  if (!['EU_GDPR_STRICT', 'US_DATA_RESIDENCY', 'GLOBAL_REPLICATED'].includes(task.policy)) throw new Error('Invalid task policy');
  if (typeof task.description !== 'string' || task.description.length > 4000) throw new Error('Invalid task description');
  requiredText(task.rule, 'task scope', 256);
}

export function loadDemoTasks(storage) {
  try {
    const raw = storage.getItem(DEMO_TASK_KEY);
    if (raw === null) return { tasks: INITIAL_DEMO_TASKS, error: null };
    if (raw.length > 1_000_000) throw new Error('Stored demo tasks exceed the size limit');
    const saved = JSON.parse(raw);
    if (saved.version !== 1 || !Array.isArray(saved.tasks) || saved.tasks.length > MAX_DEMO_TASKS) throw new Error('Unsupported demo task storage');
    saved.tasks.forEach(validateDemoTask);
    if (new Set(saved.tasks.map(task => task.id)).size !== saved.tasks.length) throw new Error('Duplicate demo task IDs');
    return { tasks: saved.tasks, error: null };
  } catch {
    return { tasks: INITIAL_DEMO_TASKS, error: 'Demo task storage is unavailable or invalid; existing data was not overwritten.' };
  }
}

export function saveDemoTask(storage, existing, task) {
  validateDemoTask(task);
  if (existing.length >= MAX_DEMO_TASKS || existing.some(item => item.id === task.id)) throw new Error('Demo task capacity or duplicate ID');
  const tasks = [...existing, task];
  storage.setItem(DEMO_TASK_KEY, JSON.stringify({ version: 1, tasks }));
  return tasks;
}
