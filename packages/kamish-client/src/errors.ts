export type ErrorCode =
  | 'VALIDATION' | 'CONNECTION' | 'TIMEOUT' | 'ABORTED' | 'CLOSED'
  | 'CAPACITY' | 'PROTOCOL' | 'BROKER_REJECTED' | 'PRODUCE_PENDING';

export class KamishError extends Error {
  constructor(
    message: string,
    public readonly code: ErrorCode,
    /** A write may have committed. Never interpret this as a safe rejection. */
    public readonly outcomeUnknown = false,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = new.target.name;
  }
}

export class ValidationError extends KamishError {
  constructor(message: string) { super(message, 'VALIDATION'); }
}
export class ConnectionError extends KamishError {
  constructor(outcomeUnknown = false, cause?: unknown) {
    super('Broker connection failed', 'CONNECTION', outcomeUnknown, { cause });
  }
}
export class TimeoutError extends KamishError {
  constructor(outcomeUnknown = false) {
    super('Broker request timed out', 'TIMEOUT', outcomeUnknown);
  }
}
export class AbortError extends KamishError {
  constructor(outcomeUnknown = false) {
    super('Broker request aborted', 'ABORTED', outcomeUnknown);
  }
}
export class ClientClosedError extends KamishError {
  constructor(outcomeUnknown = false) {
    super('Client is closed', 'CLOSED', outcomeUnknown);
  }
}
export class CapacityError extends KamishError {
  constructor() { super('Client concurrency or producer state/queue limit reached', 'CAPACITY'); }
}
export class ProtocolError extends KamishError {
  constructor(message: string, outcomeUnknown = false, cause?: unknown) {
    super(message, 'PROTOCOL', outcomeUnknown, { cause });
  }
}
export class BrokerRejectedError extends KamishError {
  constructor(public readonly reason: string, outcomeUnknown = false) {
    super(`Broker rejected operation: ${reason}`, 'BROKER_REJECTED', outcomeUnknown);
  }
}
export class PendingProduceError extends KamishError {
  constructor() {
    super('This stream has an unresolved write; call retryPending before sending another record',
      'PRODUCE_PENDING', true);
  }
}
