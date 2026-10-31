import { Socket } from 'node:net';
import {
  AbortError, CapacityError, ClientClosedError, ConnectionError, ProtocolError, TimeoutError,
} from './errors.js';
import type { KamishError } from './errors.js';
import type { RequestOptions } from './types.js';
import { integer } from './protocol.js';

export interface TransportOptions {
  host: string;
  port: number;
  connectTimeoutMs: number;
  requestTimeoutMs: number;
  maxResponseBytes: number;
  maxConcurrentRequests: number;
}

/** One request per socket: the alpha wire protocol has no correlation IDs. */
export class Transport {
  private closed = false;
  private readonly active = new Map<Socket, () => void>();

  constructor(private readonly options: TransportOptions) {}

  close(): void {
    this.closed = true;
    for (const cancel of this.active.values()) cancel();
  }

  async request(frame: Buffer, write: boolean, options: RequestOptions): Promise<Buffer> {
    const timeout = options.timeoutMs ?? this.options.requestTimeoutMs;
    integer(timeout, 'timeoutMs', 1, 0x7fff_ffff);
    if (this.closed) throw new ClientClosedError();
    if (options.signal?.aborted) throw new AbortError();
    if (this.active.size >= this.options.maxConcurrentRequests) throw new CapacityError();

    return new Promise<Buffer>((resolve, reject) => {
      const socket = new Socket();
      let settled = false;
      let sent = false;
      let used = 0;
      let buffer = Buffer.alloc(Math.min(4096, this.options.maxResponseBytes));
      const unknown = () => write && sent;
      const finish = (error?: KamishError, result?: Buffer) => {
        if (settled) return;
        settled = true;
        clearTimeout(deadline);
        clearTimeout(connectDeadline);
        options.signal?.removeEventListener('abort', abort);
        this.active.delete(socket);
        socket.destroy();
        if (error !== undefined) reject(error);
        else resolve(result!);
      };
      const abort = () => finish(new AbortError(unknown()));
      const deadline = setTimeout(() => finish(new TimeoutError(unknown())), timeout);
      const connectDeadline = setTimeout(() => finish(new TimeoutError(false)), this.options.connectTimeoutMs);
      this.active.set(socket, () => finish(new ClientClosedError(unknown())));
      options.signal?.addEventListener('abort', abort, { once: true });

      socket.setNoDelay(true);
      socket.on('error', (cause) => finish(new ConnectionError(unknown(), cause)));
      socket.on('end', () => finish(new ProtocolError('Broker closed before a complete response frame', unknown())));
      socket.on('close', () => finish(new ConnectionError(unknown())));
      socket.on('data', (chunk: Buffer) => {
        if (settled) return;
        const newline = chunk.indexOf(10);
        const length = newline === -1 ? chunk.length : newline;
        if (used + length > this.options.maxResponseBytes) {
          finish(new ProtocolError('Response exceeds maxResponseBytes', unknown()));
          return;
        }
        if (newline !== -1 && chunk.subarray(newline + 1).toString().trim() !== '') {
          finish(new ProtocolError('Unexpected extra response frame', unknown()));
          return;
        }
        if (used + length > buffer.length) {
          const larger = Buffer.alloc(Math.min(this.options.maxResponseBytes,
            Math.max(used + length, buffer.length * 2)));
          buffer.copy(larger, 0, 0, used);
          buffer = larger;
        }
        chunk.copy(buffer, used, 0, length);
        used += length;
        if (newline !== -1) finish(undefined, buffer.subarray(0, used));
      });
      socket.once('connect', () => {
        if (settled) return;
        clearTimeout(connectDeadline);
        // Set before write: even an immediate socket error can have an unknown
        // outcome once handing the request to the transport has been attempted.
        sent = true;
        socket.write(frame);
      });
      try { socket.connect(this.options.port, this.options.host); }
      catch (cause) { finish(new ConnectionError(false, cause)); }
    });
  }
}
