# Work queue stream

Background task queue for long-running daemon work — enqueue and return immediately, drain on a background runner.

| Document | Role |
|----------|------|
| [RESULTS.md](RESULTS.md) | `queue_latency` bench — enqueue vs synchronous reconcile_walk |

**Code:** `src/domain/queue.rs`, `src/ports/queue.rs`, `src/adapters/queue.rs`, `src/queue/`, `benches/queue_latency.rs`.

**Landed (2026-08-04):** FIFO `QueueStore` port + `InMemoryQueueStore`, `WorkQueue` orchestrator, daemon runner with self-rescheduling recurring tasks, `reconcile_walk` safety net (dimension 4 partial), `vault status` queue snapshot via `queue.json`.
