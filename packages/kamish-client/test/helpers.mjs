import { createServer } from 'node:net';

export function json(value) {
  return JSON.stringify(value, (_key, item) =>
    typeof item === 'bigint' ? JSON.rawJSON(item.toString()) : item);
}

export function reply(socket, value) { socket.end(`${json(value)}\n`); }

export function ack(offset = 0n) {
  return { status: 'Produce', payload: {
    base_offset: offset, count: 1, committed_epoch: 1n,
    sovereignty_allowed: true, denial_reason: 'ok',
  } };
}

export function record(offset = 0n) {
  return { magic: 0x4859, epoch: 1n, class_id: 'general', policy_version: 1n,
    base_offset: offset, payload: [0, 128, 255], checksum: 123 };
}

export const send = { topic: 'events', partition: 0, classId: 'general',
  payload: Uint8Array.from([0, 128, 255]) };

export const produce = { ...send, producerId: 42n, producerEpoch: 1n, sequence: 0 };

export async function mockBroker(t, handler) {
  const sockets = new Set();
  const server = createServer((socket) => {
    sockets.add(socket);
    socket.on('error', () => {});
    socket.once('close', () => sockets.delete(socket));
    let received = '';
    socket.on('data', (chunk) => {
      received += chunk.toString();
      const newline = received.indexOf('\n');
      if (newline !== -1) {
        const line = received.slice(0, newline);
        received = received.slice(newline + 1);
        handler(JSON.parse(line), socket, line);
      }
    });
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  t.after(async () => {
    for (const socket of sockets) socket.destroy();
    await new Promise((resolve) => server.close(resolve));
  });
  return { allowInsecure: true, host: '127.0.0.1', port: server.address().port,
    connectTimeoutMs: 500, requestTimeoutMs: 1000 };
}
