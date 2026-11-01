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

  async joinGroup(request: import('./types.js').JoinGroupRequest, options: RequestOptions = {}): Promise<import('./types.js').JoinGroupResult> {
    const wire = {
      action: 'JoinGroup',
      payload: {
        group_id: request.groupId,
        member_id: request.memberId ?? null,
        client_id: request.clientId,
        client_host: request.clientHost ?? null,
        protocol_type: request.protocolType ?? 'consumer',
        session_timeout_ms: request.sessionTimeoutMs ?? 10000,
        subscribed_topics: Array.from(request.subscribedTopics),
      },
    };
    const resp = await this.request(wire, true, options);
    if (resp.status === 'GroupJoined' && typeof resp.payload === 'object' && resp.payload !== null) {
      const p = resp.payload as Record<string, unknown>;
      return {
        groupId: String(p.group_id),
        memberId: String(p.member_id),
        generationId: Number(p.generation_id),
        leaderId: String(p.leader_id),
        state: String(p.state),
        assignedPartitions: (p.assigned_partitions ?? {}) as Record<string, number[]>,
      };
    }
    throw new ProtocolError(`Unexpected joinGroup response: ${JSON.stringify(resp)}`);
  }

  async heartbeatGroup(request: import('./types.js').HeartbeatGroupRequest, options: RequestOptions = {}): Promise<void> {
    const wire = {
      action: 'HeartbeatGroup',
      payload: {
        group_id: request.groupId,
        member_id: request.memberId,
        generation_id: request.generationId,
      },
    };
    const resp = await this.request(wire, false, options);
    if (resp.status !== 'GroupHeartbeat') {
      throw new ProtocolError(`Unexpected heartbeatGroup response: ${JSON.stringify(resp)}`);
    }
  }

  async leaveGroup(request: import('./types.js').LeaveGroupRequest, options: RequestOptions = {}): Promise<void> {
    const wire = {
      action: 'LeaveGroup',
      payload: {
        group_id: request.groupId,
        member_id: request.memberId,
      },
    };
    const resp = await this.request(wire, true, options);
    if (resp.status !== 'GroupLeft') {
      throw new ProtocolError(`Unexpected leaveGroup response: ${JSON.stringify(resp)}`);
    }
  }

  async commitOffset(request: import('./types.js').CommitOffsetRequest, options: RequestOptions = {}): Promise<bigint> {
    const wire = {
      action: 'CommitOffset',
      payload: {
        group_id: request.groupId,
        topic: request.topic,
        partition: request.partition,
        offset: Number(request.offset),
        generation_id: request.generationId ?? null,
        member_id: request.memberId ?? null,
      },
    };
    const resp = await this.request(wire, true, options);
    if (resp.status === 'OffsetCommitted' && typeof resp.payload === 'object' && resp.payload !== null) {
      return BigInt((resp.payload as { offset: number }).offset);
    }
    throw new ProtocolError(`Unexpected commitOffset response: ${JSON.stringify(resp)}`);
  }

  async fetchOffset(request: import('./types.js').FetchOffsetRequest, options: RequestOptions = {}): Promise<bigint | null> {
    const wire = {
      action: 'FetchOffset',
      payload: {
        group_id: request.groupId,
        topic: request.topic,
        partition: request.partition,
      },
    };
    const resp = await this.request(wire, false, options);
    if (resp.status === 'OffsetFetched' && typeof resp.payload === 'object' && resp.payload !== null) {
      const off = (resp.payload as { offset: number | null }).offset;
      return off !== null && off !== undefined ? BigInt(off) : null;
    }
    throw new ProtocolError(`Unexpected fetchOffset response: ${JSON.stringify(resp)}`);
  }

  async getWatermark(request: WatermarkRequest, options: RequestOptions = {}): Promise<bigint> {
    return watermarkResult(await this.request(watermarkWire(request), false, options));
  }

  private async request(wire: object, write: boolean, options: RequestOptions): Promise<Record<string, unknown>> {
    const frame = encode(wire, this.maxRequestBytes);
    return decode(await this.transport.request(frame, write, { ...options }));
  }
}
