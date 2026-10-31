import { BrokerRejectedError, ProtocolError, ValidationError } from './errors.js';
import type {
  FetchRequest, FetchResult, KamishRecord, ProduceRequest, ProduceResult,
  RecordContext, ResidencyPolicy, SendRequest, StreamRef, WatermarkRequest,
} from './types.js';

export const U64_MAX = (1n << 64n) - 1n;
export const U32_MAX = 0xffff_ffff;

// Node 22 implements these JSON APIs; TS 5.9's standard library does not yet
// declare them. Keep the runtime-specific typing local rather than changing
// consumers' global JSON declarations.
const sourceJSON = JSON as unknown as {
  rawJSON(text: string): unknown;
  parse(text: string, reviver: (key: string, value: unknown, context: { source?: string }) => unknown): unknown;
};

export function integer(value: number, name: string, min = 0, max = U32_MAX): void {
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    throw new ValidationError(`${name} must be an integer between ${min} and ${max}`);
  }
}

export function u64(value: bigint, name: string): void {
  if (typeof value !== 'bigint' || value < 0n || value > U64_MAX) {
    throw new ValidationError(`${name} must be an unsigned 64-bit bigint`);
  }
}

function text(value: string, name: string, max = 249): void {
  if (typeof value !== 'string' || value.length === 0 || Buffer.byteLength(value) > max) {
    throw new ValidationError(`${name} must be a nonempty string of at most ${max} UTF-8 bytes`);
  }
}

export function stream(value: StreamRef): void {
  text(value.topic, 'topic');
  if (!/^[A-Za-z0-9._-]+$/.test(value.topic) || value.topic === '.' || value.topic === '..') {
    throw new ValidationError('topic must be a safe single path component');
  }
  integer(value.partition, 'partition');
}

function residency(value: ResidencyPolicy | undefined): string | Record<string, string[]> | null {
  if (value === undefined) return null;
  switch (value.type) {
    case 'StrictLocal':
    case 'GlobalCleartext': return value.type;
    case 'RestrictedReplication':
    case 'SealedCiphertext': {
      if (!Array.isArray(value.regions) || value.regions.length > 256) {
        throw new ValidationError('regions must be an array of at most 256 region IDs');
      }
      for (const region of value.regions) text(region, 'region');
      return { [value.type]: [...value.regions] };
    }
    default: throw new ValidationError('Unknown residency policy');
  }
}

/** Validate and detach caller-owned mutable inputs before asynchronous queuing. */
export function snapshotSend(value: SendRequest, maxPayloadBytes: number): SendRequest {
  stream(value);
  text(value.classId, 'classId');
  if (!(value.payload instanceof Uint8Array) || value.payload.byteLength > maxPayloadBytes) {
    throw new ValidationError(`payload must be a Uint8Array of at most ${maxPayloadBytes} bytes`);
  }
  residency(value.residency);
  const copy: SendRequest = {
    topic: value.topic, partition: value.partition, classId: value.classId,
    payload: Uint8Array.from(value.payload),
  };
  if (value.residency !== undefined) {
    copy.residency = 'regions' in value.residency
      ? { type: value.residency.type, regions: [...value.residency.regions] }
      : { type: value.residency.type };
  }
  return copy;
}

export function produceWire(value: ProduceRequest): object {
  u64(value.producerId, 'producerId');
  u64(value.producerEpoch, 'producerEpoch');
  integer(value.sequence, 'sequence');
  return { action: 'Produce', payload: {
    topic: value.topic, partition: value.partition, class_id: value.classId,
    producer_id: value.producerId, producer_epoch: value.producerEpoch,
    sequence_base: value.sequence, payload: Array.from(value.payload),
    residency_rule: residency(value.residency),
  } };
}

export function fetchWire(value: FetchRequest): object {
  stream(value);
  text(value.classId, 'classId');
  u64(value.startOffset, 'startOffset');
  integer(value.maxRecords ?? 100, 'maxRecords', 1, 10_000);
  return { action: 'Fetch', payload: {
    topic: value.topic, partition: value.partition, class_id: value.classId,
    start_offset: value.startOffset, max_records: value.maxRecords ?? 100,
  } };
}

export function watermarkWire(value: WatermarkRequest): object {
  stream(value);
  text(value.sourceRegion, 'sourceRegion');
  return { action: 'GetWatermark', payload: {
    topic: value.topic, partition: value.partition, source_region: value.sourceRegion,
  } };
}

export function encode(value: object, limit: number): Buffer {
  // Node 22's rawJSON preserves Rust u64 values as JSON numeric tokens, not
  // quoted strings (which serde's u64 deserializer does not accept).
  const json = JSON.stringify(value, (_key, item: unknown) =>
    typeof item === 'bigint' ? sourceJSON.rawJSON(item.toString()) : item);
  const bytes = Buffer.from(`${json}\n`);
  if (bytes.length > limit) throw new ValidationError('Encoded request exceeds maxRequestBytes');
  return bytes;
}

export function decode(frame: Buffer): Record<string, unknown> {
  try {
    const json = new TextDecoder('utf-8', { fatal: true }).decode(frame);
    const value: unknown = sourceJSON.parse(json, (_key, item, context) => {
      if (typeof item !== 'number') return item;
      const source = context.source;
      if (source === undefined || !/^(0|[1-9][0-9]*)$/.test(source)) {
        throw new Error('Expected an unsigned integer token');
      }
      return BigInt(source);
    });
    return object(value);
  } catch (cause) {
    throw new ProtocolError('Invalid broker JSON response', false, cause);
  }
}

function object(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new ProtocolError('Expected a broker response object');
  }
  return value as Record<string, unknown>;
}
function string(value: unknown): string {
  if (typeof value !== 'string') throw new ProtocolError('Expected a string response field');
  return value;
}
function unsigned(value: unknown, max = U64_MAX): bigint {
  if (typeof value !== 'bigint' || value < 0n || value > max) {
    throw new ProtocolError('Invalid unsigned integer response field');
  }
  return value;
}
function small(value: unknown, max = U32_MAX): number {
  return Number(unsigned(value, BigInt(max)));
}
function strings(value: unknown): string[] {
  if (!Array.isArray(value)) throw new ProtocolError('Expected an array response field');
  return value.map(string);
}

export function response(value: Record<string, unknown>, expected: string, write = false): Record<string, unknown> {
  if (value.status === 'Error') throw new BrokerRejectedError(string(value.payload), write);
  if (value.status !== expected) throw new ProtocolError(`Expected ${expected} response`);
  return expected === 'Pong' ? {} : object(value.payload);
}

export function produceResult(value: Record<string, unknown>): ProduceResult {
  const body = response(value, 'Produce', true);
  if (typeof body.sovereignty_allowed !== 'boolean') {
    throw new ProtocolError('Invalid produce authorization response');
  }
  const reason = string(body.denial_reason);
  if (!body.sovereignty_allowed) throw new BrokerRejectedError(reason);
  const count = small(body.count);
  if (count !== 1) throw new ProtocolError('Current protocol must acknowledge exactly one record');
  return { baseOffset: unsigned(body.base_offset), committedEpoch: unsigned(body.committed_epoch), count, reason };
}

function record(value: unknown, maxPayloadBytes: number): KamishRecord {
  const body = object(value);
  if (!Array.isArray(body.payload) || body.payload.length > maxPayloadBytes) {
    throw new ProtocolError('Invalid or oversized record payload');
  }
  const magic = small(body.magic, 0xffff);
  if (magic !== 0x4859 && magic !== 0x485a) throw new ProtocolError('Unsupported record format');
  const result: KamishRecord = {
    magic, epoch: unsigned(body.epoch), classId: string(body.class_id),
    policyVersion: unsigned(body.policy_version), baseOffset: unsigned(body.base_offset),
    payload: Uint8Array.from(body.payload.map((byte: unknown) => small(byte, 255))),
    checksum: small(body.checksum),
  };
  if (body.context !== undefined && body.context !== null) {
    const ctx = object(body.context);
    const context: RecordContext = {
      producerId: unsigned(ctx.producer_id), producerEpoch: unsigned(ctx.producer_epoch),
      sequence: small(ctx.sequence), originRegion: string(ctx.origin_region),
      requestPolicy: string(ctx.request_policy), replicationTargets: strings(ctx.replication_targets),
    };
    result.context = context;
  }
  if (magic === 0x485a && result.context === undefined) {
    throw new ProtocolError('Record format 2 requires admission context');
  }
  return result;
}

export function fetchResult(value: Record<string, unknown>, request: FetchRequest, maxPayloadBytes: number): FetchResult {
  const body = response(value, 'Fetch');
  const highWatermark = unsigned(body.high_watermark);
  if (!Array.isArray(body.records) || body.records.length > (request.maxRecords ?? 100)) {
    throw new ProtocolError('Invalid fetch record count');
  }
  const records = body.records.map((item: unknown) => record(item, maxPayloadBytes));
  let next = request.startOffset;
  for (const item of records) {
    if (item.baseOffset !== next || item.baseOffset >= highWatermark || item.classId !== request.classId) {
      throw new ProtocolError('Fetch violated the requested class, ordering, or high-water mark');
    }
    next++;
  }
  return { highWatermark, records };
}

export function watermarkResult(value: Record<string, unknown>): bigint {
  return unsigned(response(value, 'Watermark').durable_watermark);
}
