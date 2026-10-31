import { randomBytes } from 'node:crypto';
import { CapacityError, KamishError, PendingProduceError, ValidationError } from './errors.js';
import { integer, snapshotSend, stream, u64, U32_MAX } from './protocol.js';
import type { KamishClient } from './client.js';
import type {
  ProducerOptions, ProduceRequest, ProduceResult, RequestOptions, SendRequest, StreamRef,
} from './types.js';

interface StreamState {
  next: number;
  tail: Promise<void>;
  pending: ProduceRequest | undefined;
}

/** Local session helper. It does not provide global/external-effect exactly-once delivery. */
export class KamishProducer {
  readonly producerId: bigint;
  readonly producerEpoch: bigint;
  private readonly states = new Map<string, StreamState>();
  private readonly maxQueuedSends: number;
  private readonly maxStreams: number;
  private queued = 0;

  constructor(private readonly client: KamishClient, options: ProducerOptions = {}) {
    this.producerId = options.producerId ?? randomBytes(8).readBigUInt64BE();
    this.producerEpoch = options.producerEpoch ?? 1n;
    u64(this.producerId, 'producerId');
    u64(this.producerEpoch, 'producerEpoch');
    this.maxQueuedSends = options.maxQueuedSends ?? 128;
    integer(this.maxQueuedSends, 'maxQueuedSends', 1, 10_000);
    this.maxStreams = options.maxStreams ?? 128;
    integer(this.maxStreams, 'maxStreams', 1, 10_000);
  }

  async send(request: SendRequest, options: RequestOptions = {}): Promise<ProduceResult> {
    const snapshot = snapshotSend(request, this.client.maxPayloadBytes);
    const state = this.state(snapshot);
    const requestOptions = { ...options };
    return this.enqueue(state, async () => {
      if (state.pending !== undefined) throw new PendingProduceError();
      if (state.next > U32_MAX) throw new ValidationError('Sequence exhausted; create a new producer epoch');
      const produce: ProduceRequest = { ...snapshot, producerId: this.producerId,
        producerEpoch: this.producerEpoch, sequence: state.next };
      return this.attempt(state, produce, requestOptions);
    });
  }

  /** Explicitly resend the exact unresolved request, retaining identity and bytes. */
  async retryPending(ref: StreamRef, options: RequestOptions = {}): Promise<ProduceResult> {
    const state = this.state(ref);
    const requestOptions = { ...options };
    return this.enqueue(state, async () => {
      if (state.pending === undefined) throw new ValidationError('No unresolved produce for this stream');
      return this.attempt(state, state.pending, requestOptions, true);
    });
  }

  hasPending(ref: StreamRef): boolean {
    stream(ref);
    return this.states.get(JSON.stringify([ref.topic, ref.partition]))?.pending !== undefined;
  }

  private state(ref: StreamRef): StreamState {
    stream(ref);
    const key = JSON.stringify([ref.topic, ref.partition]);
    let state = this.states.get(key);
    if (state === undefined) {
      if (this.states.size >= this.maxStreams) throw new CapacityError();
      state = { next: 0, tail: Promise.resolve(), pending: undefined };
      this.states.set(key, state);
    }
    return state;
  }

  private async enqueue<T>(state: StreamState, operation: () => Promise<T>): Promise<T> {
    if (this.queued >= this.maxQueuedSends) throw new CapacityError();
    this.queued++;
    const result = state.tail.then(operation);
    state.tail = result.then(() => {}, () => {});
    try { return await result; }
    finally { this.queued--; }
  }

  private async attempt(
    state: StreamState, request: ProduceRequest, options: RequestOptions, wasPending = false,
  ): Promise<ProduceResult> {
    try {
      const result = await this.client.produce(request, options);
      state.next = request.sequence + 1;
      state.pending = undefined;
      return result;
    } catch (error) {
      // A later policy rejection of a retry does NOT prove the first attempt
      // never committed. Retain uncertainty until an actual ACK is received.
      if (wasPending || !(error instanceof KamishError) || error.outcomeUnknown) {
        state.pending = request;
      }
      if (wasPending && error instanceof KamishError && !error.outcomeUnknown) {
        throw new KamishError(error.message, error.code, true, { cause: error });
      }
      throw error;
    }
  }
}
