/** Known region names or a deployment-defined region ID. Never a geolocation proof. */
export type RegionId = string;

/** Producer restrictions only narrow server policy; they do not authorize a write. */
export type ResidencyPolicy =
  | { type: 'StrictLocal' }
  | { type: 'GlobalCleartext' }
  | { type: 'RestrictedReplication'; regions: readonly RegionId[] }
  | { type: 'SealedCiphertext'; regions: readonly RegionId[] };

export interface ClientOptions {
  /** Required acknowledgement that the current broker protocol has no TLS/authentication. */
  allowInsecure: true;
  host?: string;
  port?: number;
  connectTimeoutMs?: number;
  requestTimeoutMs?: number;
  maxRequestBytes?: number;
  maxResponseBytes?: number;
  maxPayloadBytes?: number;
  maxConcurrentRequests?: number;
}

export interface RequestOptions {
  signal?: AbortSignal;
  /** Deadline from socket dispatch, including connection and response. */
  timeoutMs?: number;
}

export interface StreamRef {
  topic: string;
  partition: number;
}

export interface SendRequest extends StreamRef {
  classId: string;
  payload: Uint8Array;
  residency?: ResidencyPolicy;
}

export interface ProduceRequest extends SendRequest {
  producerId: bigint;
  producerEpoch: bigint;
  sequence: number;
}

export interface ProduceResult {
  baseOffset: bigint;
  count: number;
  committedEpoch: bigint;
  reason: string;
}

export interface FetchRequest extends StreamRef {
  classId: string;
  startOffset: bigint;
  maxRecords?: number;
}

export interface RecordContext {
  producerId: bigint;
  producerEpoch: bigint;
  sequence: number;
  originRegion: RegionId;
  requestPolicy: string;
  replicationTargets: readonly RegionId[];
}

export interface KamishRecord {
  magic: number;
  epoch: bigint;
  classId: string;
  policyVersion: bigint;
  baseOffset: bigint;
  payload: Uint8Array;
  /** Transport decoding does not independently verify this broker checksum. */
  checksum: number;
  context?: RecordContext;
}

export interface FetchResult {
  highWatermark: bigint;
  records: KamishRecord[];
}

export interface WatermarkRequest extends StreamRef {
  sourceRegion: RegionId;
}

export interface ProducerOptions {
  /** Defaults to a cryptographically random u64. Persist explicitly if resuming a session. */
  producerId?: bigint;
  producerEpoch?: bigint;
  maxQueuedSends?: number;
  /** Sequence/pending state is never silently evicted. New streams reject at this bound. */
  maxStreams?: number;
}

export interface JoinGroupRequest {
  groupId: string;
  memberId?: string;
  clientId: string;
  clientHost?: string;
  protocolType?: string;
  sessionTimeoutMs?: number;
  subscribedTopics: readonly string[];
}

export interface JoinGroupResult {
  groupId: string;
  memberId: string;
  generationId: number;
  leaderId: string;
  state: string;
  assignedPartitions: Record<string, number[]>;
}

export interface HeartbeatGroupRequest {
  groupId: string;
  memberId: string;
  generationId: number;
}

export interface LeaveGroupRequest {
  groupId: string;
  memberId: string;
}

export interface CommitOffsetRequest extends StreamRef {
  groupId: string;
  offset: bigint;
  generationId?: number;
  memberId?: string;
}

export interface FetchOffsetRequest extends StreamRef {
  groupId: string;
}

export interface GroupSummary {
  groupId: string;
  state: string;
  generationId: number;
  memberCount: number;
  leaderId: string | null;
  subscribedTopics: string[];
}
