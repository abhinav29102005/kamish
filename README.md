**KAMISH**

Sovereignty-aware, WAN-adaptive multi-region event streaming.
One signed policy governs every byte movement, every route and every failover.


What it does:
Enforces a signed, versioned residency policy (Ed25519) at every point where data is stored or sent
Writes checksummed, fsynced local logs with idempotent producer retries
Replicates offset-preserving, one-record ranges to policy-permitted peers
Keeps a SHA-256 hash-chained, payload-free audit log
Filters routes by policy first, then scores them (RTT, loss, queue, cost, risk)
