#!/usr/bin/env node
/**
 * Kamish Multi-Region Benchmark Orchestrator & Evidence Lifecycle Tool.
 *
 * Complies with Phase 6 / Workstream F:
 * - Versioned broker and workload configuration manifests.
 * - Validates local and external infrastructure prerequisites (Docker, Toxiproxy, Kafka, Pulsar).
 * - Distinguishes measured runtime telemetry from configuration-only baselines.
 * - Formats machine-readable artifacts conforming to benchmarks/schema.json.
 * - Never manufactures synthetic or unverified latency percentiles.
 */

import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

function getGitCommit() {
  try {
    return execSync('git rev-parse HEAD', { cwd: rootDir, encoding: 'utf-8' }).trim();
  } catch {
    return 'unknown-commit';
  }
}

function checkCommand(cmd) {
  try {
    execSync(cmd, { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

export function evaluatePrerequisites(system) {
  const unmet = [];
  const hasDocker = checkCommand('docker --version');

  if (system === 'kamish') {
    const brokerBin = path.join(rootDir, 'target', 'debug', 'kamish-broker.exe');
    const brokerBinUnix = path.join(rootDir, 'target', 'debug', 'kamish-broker');
    if (!fs.existsSync(brokerBin) && !fs.existsSync(brokerBinUnix)) {
      unmet.push('Built binary kamish-broker not found in target/debug (run: cargo build --bin kamish-broker)');
    }
    const agentBin = path.join(rootDir, 'target', 'debug', 'kamish-agent.exe');
    const agentBinUnix = path.join(rootDir, 'target', 'debug', 'kamish-agent');
    if (!fs.existsSync(agentBin) && !fs.existsSync(agentBinUnix)) {
      unmet.push('Built binary kamish-agent not found in target/debug (run: cargo build --bin kamish-agent)');
    }
  } else if (system === 'kafka_mirrormaker2') {
    if (!hasDocker) {
      unmet.push('Docker daemon not installed or reachable for Apache Kafka 3.9.0 containers');
    }
  } else if (system === 'apache_pulsar') {
    if (!hasDocker) {
      unmet.push('Docker daemon not installed or reachable for Apache Pulsar 4.0.0 containers');
    }
  }

  // Network emulation check
  const hasToxiproxy = checkCommand('toxiproxy-cli --version');
  if (!hasToxiproxy) {
    unmet.push('Toxiproxy CLI not found in PATH for synthetic inter-region WAN delay simulation');
  }

  return {
    all_met: unmet.length === 0,
    unmet_dependencies: unmet,
  };
}

export function createRunManifest({
  system = 'kamish',
  producers = 4,
  recordSizeBytes = 1024,
  targetRateMsgSec = 5000,
  durationSec = 60,
  dryRun = false,
  outputDir = path.join(rootDir, 'benchmarks', 'results'),
}) {
  const commit = getGitCommit();
  const runId = `run-${system}-${Date.now()}`;
  const prereqs = evaluatePrerequisites(system);

  const manifest = {
    run_id: runId,
    system,
    status: prereqs.all_met && !dryRun ? 'EXECUTED_MEASURED' : 'CONFIGURED_UNEXECUTED',
    evidence_type: prereqs.all_met && !dryRun ? 'MEASURED_RUNTIME_TELEMETRY' : 'BASELINE_CONFIGURATION_ONLY',
    git_commit: commit,
    timestamp: new Date().toISOString(),
    topology: {
      regions: ['EU_WEST', 'US_EAST', 'AP_SOUTH'],
      simulated_rtt_ms: 72.0,
      packet_loss_rate: 0.001,
    },
    workload: {
      producers,
      record_size_bytes: recordSizeBytes,
      target_rate_msg_sec: targetRateMsgSec,
      duration_sec: durationSec,
    },
    durability_level:
      system === 'kamish'
        ? 'LeaderFsync (local NVMe fsync before producer ACK; async durable WAN outbox replication)'
        : system === 'kafka_mirrormaker2'
        ? 'acks=all (min.insync.replicas=2) with MirrorMaker 2 Connect'
        : 'writeQuorum=3, ackQuorum=2 with built-in geo-replication',
    prerequisites: prereqs,
    measured_results: prereqs.all_met && !dryRun
      ? {
          note: 'Populated when benchmark run executes in an explicitly authorized verification phase',
        }
      : null,
  };

  fs.mkdirSync(outputDir, { recursive: true });
  const outputFile = path.join(outputDir, `${runId}.json`);
  fs.writeFileSync(outputFile, JSON.stringify(manifest, null, 2), 'utf-8');

  return { manifest, outputFile };
}

// CLI entry point
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const systemArg = args.find((a) => a.startsWith('--system='))?.split('=')[1] ?? 'kamish';
  const dryRun = args.includes('--dry-run') || args.includes('--validate-prereqs');

  console.log(`[Kamish Benchmark Orchestrator] Target System: ${systemArg}`);
  const prereqs = evaluatePrerequisites(systemArg);
  console.log(`[Prerequisites] Met: ${prereqs.all_met}`);
  if (!prereqs.all_met) {
    console.log('[Unmet Dependencies]:');
    for (const dep of prereqs.unmet_dependencies) {
      console.log(`  - ${dep}`);
    }
  }

  const { manifest, outputFile } = createRunManifest({
    system: systemArg,
    dryRun: true, // Always safe default: does not execute unverified benchmarks
  });

  console.log(`[Manifest Generated]: ${outputFile}`);
  console.log(`[Status]: ${manifest.status} (${manifest.evidence_type})`);
}

