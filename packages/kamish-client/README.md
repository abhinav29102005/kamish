# Kamish client for Node.js

**Experimental, unpublished alpha: `0.1.0-alpha.1`. Node.js 22.12+; ESM.**

A TypeScript SDK for Kamish's current newline-delimited JSON/TCP protocol.
Supports produce, fetch, watermark queries, ping, explicit write retry, typed
errors, cancellation and transport bounds. No runtime npm dependencies.

`kamish-client` is a **provisional local package name**. Ownership/availability
of the npm scope has not been verified. `private: true` intentionally prevents
publication until the name and release/security gates are approved.

## Security and compatibility

- **Node.js only.** Browsers cannot use this TCP transport; React needs a backend
  gateway. This is not a Kafka-protocol client, gRPC client, or broker installer.
- **The broker transport is currently plaintext and unauthenticated.** You must
  explicitly set `allowInsecure: true`. Use synthetic data on loopback or an
  appropriately isolated test network. Never expose this protocol to the public
  internet or send credentials/sensitive data over it.
- Tested against this repository's working-tree broker after remediation stages
  1–2, including record formats v1 and v2. There is no negotiated wire version or
  guarantee of compatibility with other Kamish revisions.
- Regional quorum durability, distributed ownership, topic authorization and
  other server gaps remain open. The SDK does not repair or certify them.
- A partition's class, producer restriction and admitted destinations are fixed
  by its first local record. Changes require a new topic/stream rather than an
  offset-preserving mirror with policy-filtered holes. Rejections do not mean
  the client may silently broaden or reclassify an existing stream.
- `SealedCiphertext` is modeled for wire compatibility, but the remediated broker
  currently rejects sealed replication. It does not encrypt data in this SDK.
- Fetched checksum fields are returned but not independently verified. Structural
  checks are not transport authentication or proof of residency.

## Build and use locally

From this directory:

```bash
npm ci
npm run build
npm test
npm run test:integration  # requires Cargo/Rust and the repository
npm run test:package     # pack, install and typecheck in an isolated temp project
```

To use from another local application, first build here, then in that application:

```bash
npm install /absolute/path/to/kamish/packages/kamish-client
```

Or run `npm pack` here and install the resulting `.tgz`. Neither action publishes
to npm. The package is ESM-only; use `import`, not a CommonJS `require` interface.

## Quick start

You need a running, policy-configured broker. **The ordinary broker CLI still
starts with an empty policy cache**, so it will reject produce/fetch operations.
For synthetic local development, start the test fixture from the repository root:

```bash
cargo run -p kamish-client --example sdk_test_broker -- ./data/sdk-demo
```

It prints a JSON line with an ephemeral loopback port. This fixture uses a fixed
**test-only** signer, permits class `general` in `EU_WEST`, and has no remote peers.
It is not a production configuration example. Use its printed port below:

```ts
import { KamishClient } from 'kamish-client';

const client = new KamishClient({
  host: '127.0.0.1',
  port: 8080, // replace with the fixture's printed port
  allowInsecure: true,
  requestTimeoutMs: 5000,
});

try {
  await client.ping();
  const producer = client.createProducer(); // random u64 producer ID, epoch 1
  const ack = await producer.send({
    topic: 'orders',
    partition: 0,
    classId: 'general',
    payload: new TextEncoder().encode('hello from Node.js'),
    residency: { type: 'StrictLocal' },
  });
  console.log(ack.baseOffset); // bigint, for example 0n

  const batch = await client.fetch({
    topic: 'orders', partition: 0, classId: 'general',
    startOffset: 0n, maxRecords: 100,
  });
  for (const record of batch.records) {
    console.log(record.baseOffset, new TextDecoder().decode(record.payload));
  }

  const watermark = await client.getWatermark({
    topic: 'orders', partition: 0, sourceRegion: 'EU_WEST',
  });
  console.log(watermark);
} finally {
  client.close(); // aborts active requests and releases sockets
}
```

## API

| Method | Result / behavior |
| --- | --- |
| `client.ping(options?)` | Resolves when broker returns `Pong` |
| `client.produce(request, options?)` | Low-level produce with explicit `producerId`, `producerEpoch`, `sequence`; returns `ProduceResult` |
| `client.fetch(request, options?)` | Returns records plus `highWatermark`; does not commit consumer offsets |
| `client.getWatermark(request, options?)` | Returns the broker-reported `bigint` watermark |
| `client.createProducer(options?)` | Creates a per-topic/partition sequence helper |
| `producer.send(request, options?)` | Serializes sends within each stream; advances sequence only on ACK |
| `producer.hasPending(stream)` | Whether an uncertain produce is retained for that stream |
| `producer.retryPending(stream, options?)` | Explicitly resends the exact retained request |
| `client.close()` | Permanently closes the client; idempotent |

All network operations accept `{ signal?: AbortSignal, timeoutMs?: number }`.
No request is retried automatically. Independent requests use independent sockets
because the wire protocol has no correlation IDs. Connection pooling is deferred;
do not treat this alpha as a throughput-optimized client.

### Integers and payloads

Rust `u64` identifiers, epochs, versions and offsets use **`bigint`**, not `number`.
The codec preserves unquoted JSON integer tokens without precision loss. Partition
IDs, producer sequences, record counts and checksums use validated `number` values.
Payloads use `Uint8Array` (`Buffer` is accepted); UTF-8 encoding is caller-controlled.

If you serialize SDK results with `JSON.stringify`, supply your own bigint replacer:

```ts
JSON.stringify(result, (_key, value) => typeof value === 'bigint' ? value.toString() : value);
```

### Retry and uncertain-outcome handling

A disconnect, timeout, cancellation or malformed response **after write dispatch**
may mean the record committed but its ACK was lost. A generic broker `Error` can
also occur after commit. `KamishError.outcomeUnknown` distinguishes these from a
known pre-dispatch failure or explicit `sovereignty_allowed: false` rejection.

```ts
import { KamishError } from 'kamish-client';

try {
  await producer.send(request);
} catch (error) {
  if (error instanceof KamishError && error.outcomeUnknown) {
    // Restore connectivity/check policy before doing this. No timer retries here.
    const ack = await producer.retryPending({ topic: request.topic, partition: request.partition });
    console.log(ack.baseOffset);
  } else {
    throw error;
  }
}
```

The producer blocks subsequent sends on that stream while an uncertain request
exists. It retains a detached copy of the exact payload, identity, sequence and
restriction. A later policy rejection of the retry does not prove that the first
attempt never committed, so the pending state remains until a successful ACK.

**Limits:** producer state is in memory and is not shared between helper instances
or Node.js processes. Never use multiple helpers with the same identity on the
same stream. Restarting a helper at sequence zero is not general session recovery;
persist your own identity/sequence/outcome journal and use low-level `produce()`
when resuming. Server-side deduplication depends on the broker version and history.
No global or end-to-end exactly-once guarantee is made.

### Defaults and limits

| Option | Default |
| --- | --- |
| `host` / `port` | `127.0.0.1` / `8080` (matches broker, not the older ctl default) |
| `connectTimeoutMs` | 2,000 |
| `requestTimeoutMs` | 5,000, from socket dispatch including connection time |
| `maxRequestBytes` / `maxResponseBytes` | 8 MiB / 8 MiB |
| `maxPayloadBytes` | 1 MiB |
| `maxConcurrentRequests` | 16; excess requests reject rather than queue |
| Producer `maxQueuedSends` | 128 across streams |
| Producer `maxStreams` | 128 tracked topic/partitions; new streams reject at the bound |
| `fetch.maxRecords` | 100; accepted range 1–10,000 |

JSON byte-array framing is larger than the original payload. Reduce fetch batch
size if the response limit is exceeded. A producer's local queue waiting time is
not included in the socket deadline. Cancellation is checked again on dispatch.
Stream sequence/pending state is retained for the helper's lifetime and is never
silently evicted; `maxStreams` bounds this state. Create bounded numbers of client
and producer instances. Limits bound this client, not broker memory.

Exported errors: `ValidationError`, `ConnectionError`, `TimeoutError`, `AbortError`,
`ClientClosedError`, `CapacityError`, `ProtocolError`, `BrokerRejectedError`, and
`PendingProduceError`, all extending `KamishError`. Generic broker error strings
remain available as `BrokerRejectedError.reason`; there are no stable server error
codes to infer from those strings yet.

## Before a public alpha

- Confirm the npm scope/name and maintainer account; set repository metadata.
- Record/pin a compatible broker release and document protocol changes.
- Review outstanding server security issues and state the experimental limits.
- Run unit, Rust integration, package-consumer, and supported-Node-version tests.
- Review the tarball; configure registry authentication/2FA or trusted publishing.
- Only after approval, remove `private: true` and publish with an **alpha** tag,
  not `latest`. No publish workflow or credentials are included here.

Licensed under Apache-2.0, consistent with the Rust workspace; see `LICENSE`.
