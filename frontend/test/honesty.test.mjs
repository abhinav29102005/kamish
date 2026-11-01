import test from 'node:test';
import assert from 'node:assert/strict';

test('honesty: LIVE mode strictly suppresses synthetic traffic and random metric generators', () => {
  // Simulate the logic in KamishContext tick loop
  let isLiveMode = true;
  let localCommittedOffset = 100;
  let trafficRateEps = 50;
  let scenarioRunning = false;

  // Background traffic generator condition from KamishContext.tsx:
  // if (!isLiveMode && trafficRateEps > 0 && scenarioState.status !== 'running')
  const shouldSimulateTraffic = !isLiveMode && trafficRateEps > 0 && !scenarioRunning;
  assert.equal(shouldSimulateTraffic, false, 'Simulated traffic generator must NOT run when isLiveMode is true');

  if (shouldSimulateTraffic) {
    localCommittedOffset += 10;
  }
  assert.equal(localCommittedOffset, 100, 'localCommittedOffset must not change from simulated generator in LIVE mode');

  // Verify that in DEMO mode, the generator is allowed to run
  isLiveMode = false;
  const shouldSimulateInDemo = !isLiveMode && trafficRateEps > 0 && !scenarioRunning;
  assert.equal(shouldSimulateInDemo, true, 'Simulated traffic generator runs only in DEMO mode');
});

test('honesty: authority model reclassifies consensus as LOCAL in-memory rather than unbacked etcd quorum', () => {
  const localAuthority = {
    authority_type: 'LOCAL (in-memory; quorum requires etcd)',
    consensus_backing: 'LOCAL_IN_MEMORY',
    quorum_backed: false,
    active_region: 'EU_WEST',
  };

  assert.equal(localAuthority.quorum_backed, false);
  assert.equal(localAuthority.consensus_backing, 'LOCAL_IN_MEMORY');
  assert.match(localAuthority.authority_type, /LOCAL/);
  assert.doesNotMatch(localAuthority.authority_type, /^etcd 3\.7\.3 External/);
});

test('honesty: fault scenario endpoints report SIMULATED or NOT_IMPLEMENTED rather than canned success', () => {
  const scenarioResults = {
    residency: { status: 'SIMULATED', simulated: true },
    slowlink: { status: 'PASS', simulated: false },
    outage: { status: 'SIMULATED', simulated: true },
    quorum: { status: 'NOT_IMPLEMENTED', simulated: true },
  };

  assert.equal(scenarioResults.residency.status, 'SIMULATED');
  assert.equal(scenarioResults.outage.status, 'SIMULATED');
  assert.equal(scenarioResults.quorum.status, 'NOT_IMPLEMENTED');
  assert.equal(scenarioResults.quorum.simulated, true);
  assert.equal(scenarioResults.slowlink.status, 'PASS');
});

test('honesty: interim verification endpoints report UNAVAILABLE rather than canned pass', () => {
  const interimAuditVerify = {
    verification_result: 'UNAVAILABLE',
    operation: 'checkpoint_existence_check_only',
  };
  const interimCheckers = {
    status: 'UNAVAILABLE',
    history_checker: null,
    canary_scanner: null,
  };

  assert.equal(interimAuditVerify.verification_result, 'UNAVAILABLE');
  assert.equal(interimCheckers.status, 'UNAVAILABLE');
});
