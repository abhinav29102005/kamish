import test from 'node:test';
import assert from 'node:assert/strict';
import { policyForSelection, validateDemoPolicy } from '../src/utils/demoPolicy.js';
import { DEMO_TASK_KEY, INITIAL_DEMO_TASKS, loadDemoTasks, saveDemoTask } from '../src/utils/demoTasks.js';

function storage(initial = null) {
  let value = initial;
  return { getItem: key => { assert.equal(key, DEMO_TASK_KEY); return value; },
    setItem: (key, next) => { assert.equal(key, DEMO_TASK_KEY); value = next; } };
}

test('policy selection produces structured restrictions and malformed inputs fail closed', () => {
  const strict = policyForSelection('EU_GDPR_STRICT');
  assert.equal(strict.type, 'StrictLocal');
  validateDemoPolicy(strict);
  assert.deepEqual(policyForSelection('US_DATA_RESIDENCY').allowedRegions, ['US_EAST']);
  assert.throws(() => validateDemoPolicy('EU_GDPR_STRICT'));
  assert.throws(() => validateDemoPolicy({ type: 'typo' }));
  assert.throws(() => validateDemoPolicy({ type: 'RestrictedReplication', allowedRegions: ['unknown'] }));
  assert.throws(() => validateDemoPolicy({ type: 'SealedCiphertext', allowedRegions: ['US_EAST'] }));
});

test('demo task persists metadata and is retrievable after reload', () => {
  const local = storage();
  const { tasks } = loadDemoTasks(local);
  const task = { id: 'DEMO-test', title: 'Synthetic task', owner: 'QA', status: 'ACTIVE',
    policy: 'EU_GDPR_STRICT', description: 'Local only', rule: 'No broker verification' };
  const saved = saveDemoTask(local, tasks, task);
  assert.equal(saved.length, tasks.length + 1);
  assert.deepEqual(loadDemoTasks(local).tasks.at(-1), task);
  assert.equal(tasks.length, INITIAL_DEMO_TASKS.length);
  assert.throws(() => saveDemoTask(local, saved, task));
});

test('invalid storage is not overwritten and failed writes do not report a saved task', () => {
  const bad = storage('{invalid');
  assert.ok(loadDemoTasks(bad).error);
  assert.equal(bad.getItem(DEMO_TASK_KEY), '{invalid');
  const task = { ...INITIAL_DEMO_TASKS[0], id: 'DEMO-new' };
  const local = { setItem() { throw new Error('quota'); } };
  assert.throws(() => saveDemoTask(local, INITIAL_DEMO_TASKS, task), /quota/);
});

test('prototype catalog does not advertise disabled sealing as verified', () => {
  assert.equal(INITIAL_DEMO_TASKS.find(task => task.id === 'TSK-005').status, 'DISABLED');
  assert.ok(INITIAL_DEMO_TASKS.every(task => task.status !== 'PASS'));
});
