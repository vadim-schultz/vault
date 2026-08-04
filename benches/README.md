# Criterion benchmarks

Rust criterion benches for algorithmic cost (history depth, file count, vault count). Stress
scripts that need a real daemon live under `scripts/stress/`.

**Results and optimisation plans:** [`.plans/benches/`](../../.plans/benches/) — measured knee
points in `RESULTS.md`, proposed fixes in `optimize.plan.md`.

```bash
cargo bench --bench history_depth
cargo bench --bench file_count
cargo bench --bench vault_count
```
