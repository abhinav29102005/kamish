import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { KamishClient, BrokerRejectedError } from '../../dist/index.js';

const root = fileURLToPath(new URL('../../../../', import.meta.url));
const executable = join(root, 'target', 'debug', 'examples',
  process.platform === 'win32' ? 'sdk_test_broker.exe' : 'sdk_test_broker');

async function startBroker(t, dataDir) {
  const child = spawn(executable, [dataDir], { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] });
  let stopped;
  const stop = () => {
    stopped ??= new Promise((resolve) => {
      if (child.exitCode !== null || child.signalCode !== null) { resolve(); return; }
      const timer = setTimeout(() => child.kill('SIGKILL'), 3000);
      child.once('exit', () => { clearTimeout(timer); resolve(); });
      child.kill('SIGINT');
    });
    return stopped;
  };
  t.after(stop);
  const endpoint = await new Promise((resolve, reject) => {
    let stdout = '';
    let stderr = '';
    const timeout = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`Test broker did not become ready: ${stderr}`));
    }, 10_000);
    const fail = (error) => { clearTimeout(timeout); reject(error); };
    child.once('error', fail);
    child.once('exit', (code) => fail(new Error(`Test broker exited (${code}): ${stderr}`)));
    child.stderr.on('data', (chunk) => { stderr = (stderr + chunk).slice(-8192); });
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
      if (stdout.includes('\n')) {
        clearTimeout(timeout);
        try { resolve(JSON.parse(stdout.slice(0, stdout.indexOf('\n')))); }
        catch (error) { reject(error); }
      }
    });
  });
  const client = new KamishClient({ allowInsecure: true, ...endpoint });
  t.after(() => client.close());
  return { client, stop };
}

test('Node SDK talks to a real Rust broker and preserves retry identity across restart',
  { timeout: 180_000 }, async (t) => {
    execFileSync('cargo', ['build', '-p', 'kamish-client', '--example', 'sdk_test_broker',
      '--target-dir', join(root, 'target')], { cwd: root, timeout: 120_000, stdio: 'pipe' });
    const dataDir = await mkdtemp(join(tmpdir(), 'kamish-sdk-integration-'));
    // Explicit finally removes only the isolated directory created by this test,
    // after both broker processes have stopped.
    let first;
    let second;
    try {
      first = await startBroker(t, dataDir);
      await first.client.ping();
      const producerId = 18446744073709551615n;
      const producer = first.client.createProducer({ producerId });
      const payload = Uint8Array.from([0, 128, 255, 10, 13]);
      const base = { topic: 'sdk-events', classId: 'general', payload,
        residency: { type: 'StrictLocal' } };
      const acknowledgments = await Promise.all(Array.from({ length: 6 }, (_, i) =>
        producer.send({ ...base, partition: i % 2 })));
      assert.deepEqual(acknowledgments.map((a) => a.baseOffset), [0n, 0n, 1n, 1n, 2n, 2n]);
      for (const partition of [0, 1]) {
        const batch = await first.client.fetch({ topic: base.topic, partition,
          classId: base.classId, startOffset: 0n, maxRecords: 10 });
        assert.equal(batch.highWatermark, 3n);
        assert.deepEqual(batch.records.map((r) => r.context.sequence), [0, 1, 2]);
        assert.equal(batch.records[0].context.producerId, producerId);
        assert.deepEqual(batch.records[0].payload, payload);
      }
      const retry = { ...base, partition: 0, producerId, producerEpoch: 1n, sequence: 2 };
      assert.equal((await first.client.produce(retry)).baseOffset, 2n);
      await assert.rejects(first.client.produce({ ...retry, classId: 'unknown' }),
        (e) => e instanceof BrokerRejectedError && !e.outcomeUnknown);
      assert.equal(await first.client.getWatermark({ topic: base.topic, partition: 0,
        sourceRegion: 'EU_WEST' }), 3n);

      first.client.close();
      await first.stop();
      second = await startBroker(t, dataDir);
      assert.equal((await second.client.produce(retry)).baseOffset, 2n);
      const recovered = await second.client.fetch({ topic: base.topic, partition: 0,
        classId: 'general', startOffset: 0n });
      assert.equal(recovered.records.length, 3);
      assert.equal(recovered.highWatermark, 3n);
    } finally {
      first?.client.close();
      second?.client.close();
      await first?.stop();
      await second?.stop();
      await rm(dataDir, { recursive: true, force: true });
    }
  });
