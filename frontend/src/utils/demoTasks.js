export const DEMO_TASK_KEY = 'kamish.demo.tasks.v1';
export const MAX_DEMO_TASKS = 200;
export const TASK_STATUSES = ['PROTOTYPE', 'PARTIAL', 'DISABLED', 'ACTIVE', 'IN_PROGRESS', 'BLOCKED', 'DONE'];

export const INITIAL_DEMO_TASKS = [
  ['TSK-001', 'Local append log and checksum recovery', 'PROTOTYPE'],
  ['TSK-002', 'Local producer session deduplication', 'PROTOTYPE'],
  ['TSK-003', 'Bounded in-memory outbox and log replay', 'PROTOTYPE'],
  ['TSK-004', 'Signed class policy enforcement', 'PARTIAL'],
  ['TSK-005', 'Sealed replication — unavailable', 'DISABLED'],
  ['TSK-006', 'Policy-filtered route selection', 'PARTIAL'],
  ['TSK-007', 'In-process ownership checks — not distributed consensus', 'PARTIAL'],
  ['TSK-008', 'Region-local audit hash chain', 'PROTOTYPE'],
  ['TSK-009', 'Synthetic canary and ordering checks', 'PARTIAL'],
  ['TSK-010', 'Local benchmark harness — no live measurement in this UI', 'PROTOTYPE'],
].map(([id, title, status]) => ({
  id, title, status, owner: 'Kamish prototype team', policy: 'EU_GDPR_STRICT',
  description: 'Prototype component. Consult repository test reports for revision-specific evidence and limitations.',
  rule: 'Demo metadata, not a live verification result',
}));

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
