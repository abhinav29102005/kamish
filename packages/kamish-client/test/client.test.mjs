import test from 'node:test';
import assert from 'node:assert/strict';
import {
  KamishClient, ValidationError, ProtocolError, BrokerRejectedError,
  TimeoutError, AbortError, ClientClosedError, CapacityError,
} from '../dist/index.js';
import { ack, json, mockBroker, produce, record, reply } from './helpers.mjs';

test('requires explicit insecure opt-in and validates configuration', () => {
  assert.throws(() => new KamishClient({}), ValidationError);
  for (const config of [{ port: 0 }, { requestTimeoutMs: NaN }, { maxResponseBytes: -1 }]) {
    assert.throws(() => new KamishClient({ allowInsecure: true, ...config }), ValidationError);
  }
});

test('preserves maximum u64 identities and offsets without rounding', async (t) => {
  const max = 18446744073709551615n;
  let raw;
  const client = new KamishClient(await mockBroker(t, (_req, socket, line) => {
    raw = line;
    reply(socket, ack(max));
  }));
  t.after(() => client.close());
  const result = await client.produce({ ...produce, producerId: max, producerEpoch: max });
  assert.match(raw, /"producer_id":18446744073709551615[,}]/);
  assert.match(raw, /"producer_epoch":18446744073709551615[,}]/);
  assert.equal(result.baseOffset, max);
});

test('validates identities, topic paths and payload before opening a socket', async () => {
  const client = new KamishClient({ allowInsecure: true });
  for (const request of [
    { ...produce, producerId: 42 }, { ...produce, producerId: -1n },
    { ...produce, producerId: 1n << 64n }, { ...produce, topic: '../escape' },
    { ...produce, sequence: 2 ** 32 }, { ...produce, payload: 'not-bytes' },
  ]) await assert.rejects(client.produce(request), ValidationError);
  client.close();
});

test('serializes residency exactly as the Rust enum', async (t) => {
  const seen = [];
  const client = new KamishClient(await mockBroker(t, (req, socket) => {
    seen.push(req.payload.residency_rule);
    reply(socket, ack());
  }));
  t.after(() => client.close());
  await client.produce({ ...produce, residency: { type: 'StrictLocal' } });
  await client.produce({ ...produce, residency: { type: 'RestrictedReplication', regions: ['EU_WEST', 'custom'] } });
  assert.deepEqual(seen, ['StrictLocal', { RestrictedReplication: ['EU_WEST', 'custom'] }]);
});

test('fetch decodes binary bytes and v2 context with bigints', async (t) => {
  const offset = 9007199254740993n;
  const row = { ...record(offset), magic: 0x485a, context: {
    producer_id: 18446744073709551615n, producer_epoch: 1, sequence: 0,
    origin_region: 'EU_WEST', request_policy: '"StrictLocal"', replication_targets: [],
  } };
  const client = new KamishClient(await mockBroker(t, (_req, socket) =>
    reply(socket, { status: 'Fetch', payload: { high_watermark: offset + 1n, records: [row] } })));
  t.after(() => client.close());
  const result = await client.fetch({ topic: 'events', partition: 0, classId: 'general', startOffset: offset });
  assert.equal(result.highWatermark, offset + 1n);
  assert.equal(result.records[0].context.producerId, 18446744073709551615n);
  assert.deepEqual(result.records[0].payload, Uint8Array.from([0, 128, 255]));
});

test('fragmented response and split UTF-8 are decoded only after framing', async (t) => {
  const response = ack();
  response.payload.denial_reason = 'ok — durable';
  const bytes = Buffer.from(`${json(response)}\n`);
  const split = bytes.indexOf(Buffer.from('—')) + 1;
  const client = new KamishClient(await mockBroker(t, (_req, socket) => {
    socket.write(bytes.subarray(0, split));
    setTimeout(() => socket.end(bytes.subarray(split)), 5);
  }));
  t.after(() => client.close());
  assert.equal((await client.produce(produce)).reason, 'ok — durable');
});

test('watermark and ping use actual wire actions', async (t) => {
  const client = new KamishClient(await mockBroker(t, (req, socket) => {
    if (req.action === 'Ping') reply(socket, { status: 'Pong' });
    else {
      assert.equal(req.action, 'GetWatermark');
      assert.equal(req.payload.source_region, 'EU_WEST');
      reply(socket, { status: 'Watermark', payload: { durable_watermark: 9007199254740993n } });
    }
  }));
  t.after(() => client.close());
  await client.ping();
  assert.equal(await client.getWatermark({ topic: 'events', partition: 0, sourceRegion: 'EU_WEST' }), 9007199254740993n);
});

test('structured rejection differs from a possibly post-commit broker error', async (t) => {
  let attempts = 0;
  const client = new KamishClient(await mockBroker(t, (_req, socket) => {
    if (attempts++ === 0) reply(socket, { status: 'Produce', payload: {
      base_offset: 0, count: 0, committed_epoch: 0, sovereignty_allowed: false, denial_reason: 'Backpressure',
    } });
    else reply(socket, { status: 'Error', payload: 'lease_expired_outcome_unknown' });
  }));
  t.after(() => client.close());
  await assert.rejects(client.produce(produce), (e) => e instanceof BrokerRejectedError && !e.outcomeUnknown);
  await assert.rejects(client.produce(produce), (e) => e instanceof BrokerRejectedError && e.outcomeUnknown);
});

test('malformed, wrong-kind and incomplete responses fail safely', async (t) => {
  const responses = ['not-json\n', '{"status":"Pong"}\n', json(ack()),
    '{"status":"Produce","payload":{"sovereignty_allowed":true}}\n'];
  const client = new KamishClient(await mockBroker(t, (_req, socket) => socket.end(responses.shift())));
  t.after(() => client.close());
  for (let i = 0; i < 4; i++) {
    await assert.rejects(client.produce(produce), (e) => e instanceof ProtocolError && e.outcomeUnknown);
  }
});

test('response and encoded request limits are enforced', async (t) => {
  const options = await mockBroker(t, (_req, socket) => socket.end('x'.repeat(1025)));
  const client = new KamishClient({ ...options, maxResponseBytes: 1024 });
  t.after(() => client.close());
  await assert.rejects(client.ping(), ProtocolError);
  const small = new KamishClient({ ...options, maxRequestBytes: 8 });
  t.after(() => small.close());
  await assert.rejects(small.ping(), ValidationError);
});

test('fetch rejects class bypass, out-of-order offsets, HWM violations and invalid bytes', async (t) => {
  const rows = [
    { ...record(), class_id: 'private' }, record(2n),
    record(1n), { ...record(), payload: [256] },
  ];
  const client = new KamishClient(await mockBroker(t, (_req, socket) => reply(socket, {
    status: 'Fetch', payload: { high_watermark: 1, records: [rows.shift()] },
  })));
  t.after(() => client.close());
  for (let i = 0; i < 4; i++) {
    await assert.rejects(client.fetch({ topic: 'events', partition: 0, classId: 'general', startOffset: 0n }), ProtocolError);
  }
});

test('timeout after dispatch marks write outcome unknown and never retries', async (t) => {
  let requests = 0;
  const client = new KamishClient(await mockBroker(t, () => { requests++; }));
  t.after(() => client.close());
  await assert.rejects(client.produce(produce, { timeoutMs: 30 }), (e) => e instanceof TimeoutError && e.outcomeUnknown);
  assert.equal(requests, 1);
});

test('abort works before connection and after request dispatch', async (t) => {
  const controller = new AbortController();
  const client = new KamishClient(await mockBroker(t, () => controller.abort()));
  t.after(() => client.close());
  await assert.rejects(client.produce(produce, { signal: AbortSignal.abort() }), (e) => e instanceof AbortError && !e.outcomeUnknown);
  await assert.rejects(client.produce(produce, { signal: controller.signal }), (e) => e instanceof AbortError && e.outcomeUnknown);
});

test('close aborts active calls, limits concurrency, and is permanent', async (t) => {
  let received;
  const ready = new Promise((resolve) => { received = resolve; });
  const client = new KamishClient({ ...await mockBroker(t, () => received()), maxConcurrentRequests: 1 });
  const pending = client.produce(produce);
  const rejection = assert.rejects(pending, (e) => e instanceof ClientClosedError && e.outcomeUnknown);
  await ready;
  await assert.rejects(client.ping(), CapacityError);
  client.close();
  client.close();
  await rejection;
  await assert.rejects(client.ping(), ClientClosedError);
});

test('invalid UTF-8 and extra frames are rejected', async (t) => {
  let count = 0;
  const client = new KamishClient(await mockBroker(t, (_req, socket) => {
    if (count++ === 0) socket.end(Buffer.from([0xff, 10]));
    else socket.end('{"status":"Pong"}\n{"status":"Pong"}\n');
  }));
  t.after(() => client.close());
  await assert.rejects(client.ping(), ProtocolError);
  await assert.rejects(client.ping(), ProtocolError);
});

test('u64 response overflow is rejected rather than truncated', async (t) => {
  const client = new KamishClient(await mockBroker(t, (_req, socket) =>
    reply(socket, { status: 'Watermark', payload: { durable_watermark: 1n << 64n } })));
  t.after(() => client.close());
  await assert.rejects(client.getWatermark({ topic: 'events', partition: 0, sourceRegion: 'EU_WEST' }), ProtocolError);
});
