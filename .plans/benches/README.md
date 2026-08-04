# Benchmark & optimisation stream

Post-MVP hardening: find knee points under load, record numbers, then fix or limit bottlenecks in
a separate review pass.

| Document | Role |
|----------|------|
| [benchmark.plan.md](benchmark.plan.md) | Measurement plan — 7 load dimensions, harness design, exit criteria (complete) |
| [RESULTS.md](RESULTS.md) | Measured knee points and verdicts from criterion benches + stress scripts |
| [optimize.plan.md](optimize.plan.md) | Proposed fixes for every "needs limit" / "needs fix" verdict (draft, pending review) |
| [history_index.plan.md](history_index.plan.md) | Dimension 1 implementation chapter — index + `SELECT_TRACKED_FILES` rewrite (complete) |

**Harness code:** `benches/*.rs` (criterion), `scripts/stress/*.sh` (daemon + real filesystem),
`examples/simulate_history.rs`. Manual profiling tools — not wired into default CI.

See also [queue stream](../queue/README.md) for the background work queue (`benches/queue_latency.rs`).
