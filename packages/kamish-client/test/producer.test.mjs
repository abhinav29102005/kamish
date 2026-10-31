import test from 'node:test';
import assert from 'node:assert/strict';
import { KamishClient, BrokerRejectedError, PendingProduceError, KamishError, CapacityError, AbortError } from '../dist/index.js';
import { ack, mockBroker, reply, send } from './helpers.mjs';

test('concurrent sends serialize sequences per topic/partition, not globally', async (t) => {
  const sequences = new Map();
  const client = new KamishClient(await mockBroker(t, (req, socket) => {
    const key = `${req.payload.topic}:${req.payload.partition}`;
    const values = sequences.get(key) ?? [];
    values.push(req.payload.sequence_base);
    sequences.set(key, values);
    reply(socket, ack(BigInt(values.length - 1)));
  }));
  t.after(() => client.close());
  const producer = client.createProducer({ producerId: 42n });
  await Promise.all(Array.from({ length: 12 }, (_, i) => producer.send({ ...send, partition: i % 2 })));
  assert.deepEqual(sequences.get('events:0'), [0, 1, 2, 3, 4, 5]);
  assert.deepEqual(sequences.get('events:1'), [0, 1, 2, 3, 4, 5]);
});

test('definite rejection does not consume a sequence', async (t) => {
  const sequences = [];
  const client = new KamishClient(await mockBroker(t, (req, socket) => {
    sequences.push(req.payload.sequence_base);
    if (sequences.length === 1) reply(socket, { status: 'Produce', payload: {
      base_offset: 0, count: 0, committed_epoch: 0, sovereignty_allowed: false, denial_reason: 'Backpressure',
    } });
    else reply(socket, ack());
  }));
  t.after(() => client.close());
  const producer = client.createProducer();
  await assert.rejects(producer.send(send), BrokerRejectedError);
  assert.equal(producer.hasPending(send), false);
  await producer.send(send);
  assert.deepEqual(sequences, [0, 0]);
});

test('lost ACK blocks the stream and explicit retry resends identical bytes', async (t) => {
  const received = [];
  const client = new KamishClient(await mockBroker(t, (_req, socket, raw) => {
    received.push(raw);
    if (received.length === 1) socket.destroy();
    else reply(socket, ack());
  }));
  t.after(() => client.close());
  const producer = client.createProducer({ producerId: 42n });
  const payload = Uint8Array.from([1, 2]);
  const pending = producer.send({ ...send, payload });
  payload.fill(99);
  await assert.rejects(pending, (e) => e instanceof KamishError && e.outcomeUnknown);
  assert.equal(producer.hasPending(send), true);
  await assert.rejects(producer.send(send), PendingProduceError);
  assert.equal(received.length, 1);
  await producer.retryPending(send);
  assert.equal(received[0], received[1]);
  assert.deepEqual(JSON.parse(received[1]).payload.payload, [1, 2]);
  assert.equal(producer.hasPending(send), false);
  await producer.send(send);
  assert.equal(JSON.parse(received[2]).payload.sequence_base, 1);
});

test('later rejection cannot resolve an earlier unknown outcome', async (t) => {
  let count = 0;
  const client = new KamishClient(await mockBroker(t, (_req, socket) => {
    if (count++ === 0) socket.destroy();
    else reply(socket, { status: 'Produce', payload: {
      base_offset: 0, count: 0, committed_epoch: 0, sovereignty_allowed: false, denial_reason: 'revoked',
    } });
  }));
  t.after(() => client.close());
  const producer = client.createProducer();
  await assert.rejects(producer.send(send));
  await assert.rejects(producer.retryPending(send), (e) => e.outcomeUnknown === true);
  assert.equal(producer.hasPending(send), true);
});

test('producer queue saturation rejects excess work without allocating a sequence', async (t) => {
  let firstSocket;
  let ready;
  const received = new Promise((resolve) => { ready = resolve; });
  const sequences = [];
  const client = new KamishClient(await mockBroker(t, (req, socket) => {
    sequences.push(req.payload.sequence_base);
    if (sequences.length === 1) { firstSocket = socket; ready(); }
    else reply(socket, ack());
  }));
  t.after(() => client.close());
  const producer = client.createProducer({ maxQueuedSends: 1 });
  const first = producer.send(send);
  await received;
  await assert.rejects(producer.send(send), CapacityError);
  reply(firstSocket, ack());
  await first;
  await producer.send(send);
  assert.deepEqual(sequences, [0, 1]);
});

test('aborting before dispatch leaves sequence reusable and no pending write', async (t) => {
  let sequence;
  const client = new KamishClient(await mockBroker(t, (req, socket) => {
    sequence = req.payload.sequence_base;
    reply(socket, ack());
  }));
  t.after(() => client.close());
  const producer = client.createProducer();
  await assert.rejects(producer.send(send, { signal: AbortSignal.abort() }), AbortError);
  assert.equal(producer.hasPending(send), false);
  await producer.send(send);
  assert.equal(sequence, 0);
});

test('tracked stream capacity does not evict an existing sequence', async (t) => {
  const seen = [];
  const client = new KamishClient(await mockBroker(t, (req, socket) => {
    seen.push(req.payload.sequence_base);
    reply(socket, ack());
  }));
  t.after(() => client.close());
  const producer = client.createProducer({ maxStreams: 1 });
  await producer.send(send);
  await assert.rejects(producer.send({ ...send, partition: 1 }), CapacityError);
  await producer.send(send);
  assert.deepEqual(seen, [0, 1]);
});
