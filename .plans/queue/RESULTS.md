# Work queue latency results

Measured on one dev machine (Linux), release builds, 2026-08-04. Compares the cost of
**enqueue** (what a trigger site pays once the queue exists) against **synchronous**
`reconcile_walk` execution (what it would pay if the walk ran inline).

Harness: `cargo bench --bench queue_latency`. Fixture: vault with N tracked files on disk
and in `meta.db`, same scale as `benches/file_count.rs`.

## Summary

| Tracked files | `reconcile_walk` (sync) | `enqueue` |
|--------------:|------------------------:|----------:|
| 100 | 418 µs | 157 ns |
| 1,000 | 1.82 ms | 158 ns |
| 10,000 | 17.1 ms | 158 ns |
| 50,000 | 85.7 ms | 157 ns |

**Verdict:** enqueue is flat across 100 → 50,000 tracked files (~157 ns — a FIFO push).
Synchronous `reconcile_walk` scales with file count (walk + `list_tracked_files` diff),
matching the shape of dimension 2's baseline-walk curve at smaller absolute values because
this bench only diffs sets rather than committing.

The queue achieves the intended UX property: the trigger site returns immediately regardless
of how many files the deferred walk will eventually touch.
