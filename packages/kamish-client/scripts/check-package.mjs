import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageDir = fileURLToPath(new URL('../', import.meta.url));
const temp = await mkdtemp(join(tmpdir(), 'kamish-package-check-'));
const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
try {
  const result = execFileSync(npm, ['pack', '--json', '--ignore-scripts', '--pack-destination', temp],
    { cwd: packageDir, encoding: 'utf8', shell: process.platform === 'win32' });
  const [pack] = JSON.parse(result);
  for (const file of pack.files) {
    assert.match(file.path, /^(dist\/|README\.md$|LICENSE$|CHANGELOG\.md$|package\.json$)/);
  }
  for (const required of ['dist/index.js', 'dist/index.d.ts', 'LICENSE', 'README.md']) {
    assert.ok(pack.files.some((file) => file.path === required), `Missing ${required}`);
  }
  const consumer = join(temp, 'consumer');
  await mkdir(consumer);
  await writeFile(join(consumer, 'package.json'), JSON.stringify({
    name: 'kamish-isolated-consumer', version: '0.0.0', private: true, type: 'module',
  }));
  execFileSync(npm, ['install', '--ignore-scripts', '--no-audit', '--no-fund', join(temp, pack.filename)],
    { cwd: consumer, stdio: 'pipe', shell: process.platform === 'win32' });
  execFileSync(process.execPath, ['--input-type=module', '-e', `
    import { KamishClient, KamishProducer, ValidationError } from '@kamish/client';
    const client = new KamishClient({ allowInsecure: true });
    if (!(client.createProducer() instanceof KamishProducer)) throw new Error('Invalid export');
    client.close();
    try { new KamishClient({}); throw new Error('Expected validation'); }
    catch (error) { if (!(error instanceof ValidationError)) throw error; }
  `], { cwd: consumer, stdio: 'pipe' });
  await writeFile(join(consumer, 'consumer.mts'), `
    import { KamishClient, type SendRequest, type FetchResult } from '@kamish/client';
    const client = new KamishClient({ allowInsecure: true });
    const request: SendRequest = { topic: 'events', partition: 0, classId: 'general', payload: new Uint8Array() };
    const producer = client.createProducer({ producerId: 42n });
    const result: Promise<bigint> = producer.send(request).then(ack => ack.baseOffset);
    const records: Promise<FetchResult> = client.fetch({ ...request, startOffset: 0n });
    void result; void records;
    // @ts-expect-error u64 identifiers must not accept lossy number values.
    client.createProducer({ producerId: 42 });
  `);
  execFileSync(process.execPath, [join(packageDir, 'node_modules/typescript/lib/tsc.js'),
    '--strict', '--noEmit', '--target', 'ES2023', '--module', 'NodeNext',
    '--moduleResolution', 'NodeNext', '--typeRoots', join(packageDir, 'node_modules/@types'),
    'consumer.mts'], { cwd: consumer, stdio: 'pipe' });
  console.log(`Package verified: ${pack.filename}; ${pack.files.length} files, ${pack.size} bytes compressed.`);
  console.log('Isolated consumer ESM imports and exported TypeScript declarations passed. Nothing published.');
} finally {
  await rm(temp, { recursive: true, force: true });
}
