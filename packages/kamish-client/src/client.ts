import { ProtocolError, ValidationError } from './errors.js';
import {
  decode, encode, fetchResult, fetchWire, integer, produceResult, produceWire,
  response, snapshotSend, watermarkResult, watermarkWire,
} from './protocol.js';
import { KamishProducer } from './producer.js';
import { Transport } from './transport.js';
import type {
  ClientOptions, FetchRequest, FetchResult, ProduceRequest, ProduceResult,
  ProducerOptions, RequestOptions, WatermarkRequest,
} from './types.js';

export class KamishClient {
  readonly maxPayloadBytes: number;
  private readonly maxRequestBytes: number;
  private readonly transport: Transport;

  constructor(options: ClientOptions) {
    if (options?.allowInsecure !== true) {
      throw new ValidationError('Current broker traffic is plaintext; set allowInsecure: true only on a trusted test network');
    }
    const host = options.host ?? '127.0.0.1';
    if (typeof host !== 'string' || host.length === 0 || host.length > 253) {
      throw new ValidationError('host must be a hostname or IP address');
    }
    const resolved = {
      host, port: options.port ?? 8080,
      connectTimeoutMs: options.connectTimeoutMs ?? 2000,
      requestTimeoutMs: options.requestTimeoutMs ?? 5000,
      maxResponseBytes: options.maxResponseBytes ?? 8 * 1024 * 1024,
      maxConcurrentRequests: options.maxConcurrentRequests ?? 16,
    };
    integer(resolved.port, 'port', 1, 65535);
    integer(resolved.connectTimeoutMs, 'connectTimeoutMs', 1, 0x7fff_ffff);
    integer(resolved.requestTimeoutMs, 'requestTimeoutMs', 1, 0x7fff_ffff);
    integer(resolved.maxResponseBytes, 'maxResponseBytes', 1, 64 * 1024 * 1024);
    integer(resolved.maxConcurrentRequests, 'maxConcurrentRequests', 1, 1024);
    this.maxPayloadBytes = options.maxPayloadBytes ?? 1024 * 1024;
    this.maxRequestBytes = options.maxRequestBytes ?? 8 * 1024 * 1024;
    integer(this.maxPayloadBytes, 'maxPayloadBytes', 0, 16 * 1024 * 1024);
    integer(this.maxRequestBytes, 'maxRequestBytes', 1, 64 * 1024 * 1024);
    this.transport = new Transport(resolved);
  }

  /** Stops active requests and permanently closes this client. */
  close(): void { this.transport.close(); }

  createProducer(options: ProducerOptions = {}): KamishProducer {
    return new KamishProducer(this, options);
  }

  async ping(options: RequestOptions = {}): Promise<void> {
    response(await this.request({ action: 'Ping' }, false, options), 'Pong');
  }

  /** Low-level produce: the caller owns producer identity and per-partition sequence. */
  async produce(request: ProduceRequest, options: RequestOptions = {}): Promise<ProduceResult> {
    const snapshot = { ...snapshotSend(request, this.maxPayloadBytes),
      producerId: request.producerId, producerEpoch: request.producerEpoch, sequence: request.sequence };
    const wire = produceWire(snapshot);
    try {
      return produceResult(await this.request(wire, true, options));
    } catch (error) {
      if (error instanceof ProtocolError && !error.outcomeUnknown) {
        throw new ProtocolError(error.message, true, error);
      }
      throw error;
    }
  }

  async fetch(request: FetchRequest, options: RequestOptions = {}): Promise<FetchResult> {
    const snapshot = { ...request };
    return fetchResult(await this.request(fetchWire(snapshot), false, options), snapshot, this.maxPayloadBytes);
  }

  async getWatermark(request: WatermarkRequest, options: RequestOptions = {}): Promise<bigint> {
    return watermarkResult(await this.request(watermarkWire(request), false, options));
  }

  private async request(wire: object, write: boolean, options: RequestOptions): Promise<Record<string, unknown>> {
    const frame = encode(wire, this.maxRequestBytes);
    return decode(await this.transport.request(frame, write, { ...options }));
  }
}
