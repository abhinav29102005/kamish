export { KamishClient } from './client.js';
export { KamishProducer } from './producer.js';
export {
  KamishError, ValidationError, ConnectionError, TimeoutError, AbortError,
  ClientClosedError, CapacityError, ProtocolError, BrokerRejectedError, PendingProduceError,
} from './errors.js';
export type { ErrorCode } from './errors.js';
export type {
  ClientOptions, RequestOptions, StreamRef, RegionId, ResidencyPolicy, SendRequest,
  ProduceRequest, ProduceResult, FetchRequest, FetchResult, KamishRecord, RecordContext,
  WatermarkRequest, ProducerOptions,
} from './types.js';
