# Vault implementation plans

Four streams: the MVP bootstrap (landed), ongoing benchmark / optimisation work, release
build tuning, and CLI UX.

## MVP bootstrap (complete)

Summary, architecture reference, and archived chapter plans:

| Document | Description |
|----------|-------------|
| [mvp/README.md](mvp/README.md) | Implementation stages, status table, vision |
| [mvp/architecture.md](mvp/architecture.md) | Ports-and-adapters layout (compact reference) |
| [mvp/chapters/](mvp/chapters/) | Archived chapter plans (ch 0–5, CLI refactor, showcase) |

## Benchmark & optimisation (active)

Measurement results and proposed fixes:

| Document | Description |
|----------|-------------|
| [benches/README.md](benches/README.md) | Stream overview |
| [benches/benchmark.plan.md](benches/benchmark.plan.md) | Stress-test suite plan (complete) |
| [benches/RESULTS.md](benches/RESULTS.md) | Measured knee points and verdicts |
| [benches/optimize.plan.md](benches/optimize.plan.md) | Proposed bottleneck fixes (draft) |
| [queue/README.md](queue/README.md) | Background work queue stream |
| [queue/RESULTS.md](queue/RESULTS.md) | Enqueue vs sync reconcile_walk latency |

## Release build tuning (active)

Binary size and compilation profile — what ships, not what runs:

| Document | Description |
|----------|-------------|
| [release/README.md](release/README.md) | Stream overview |
| [release/binary_size.plan.md](release/binary_size.plan.md) | Measured profile-tuning + dependency-feature-trim proposals (draft) |

## CLI UX (active)

Humanizing the read commands — hiding commit SHAs, raw byte dumps, and other internal plumbing:

| Document | Description |
|----------|-------------|
| [cli/README.md](cli/README.md) | Stream overview |
| [cli/humanize_show_log.plan.md](cli/humanize_show_log.plan.md) | `vault show` scope levels + humanized `vault log` output (landed) |
