# Vault implementation plans

Chapter-based bootstrap plans for the Vault document version manager. Update these as chapters land.

| Plan | Description |
|------|-------------|
| [chapter_0.plan.md](chapter_0.plan.md) | Overview — vision, architecture, tech stack, chapters 0–6 |
| [chapter_1.plan.md](chapter_1.plan.md) | Empty repo, green CI — Cargo scaffold, minimal CLI, CI pipeline |
| [chapter_2.plan.md](chapter_2.plan.md) | Documentation skeleton — mdBook site, GitHub Pages |
| [chapter_3.plan.md](chapter_3.plan.md) | `vault init` + storage foundation — gix, SQLite, `.vault/` layout |
| [chapter_4.plan.md](chapter_4.plan.md) | Singleton background watcher — registry, daemon, snapshots |
| [architecture.plan.md](architecture.plan.md) | Ports-and-adapters refactor — layered crate layout |
| [chapter_5.plan.md](chapter_5.plan.md) | Time-travel read commands — show/log/diff/restore/list, gix blob reads, CLI date parsing |
| [benchmark.plan.md](benchmark.plan.md) | Benchmark & stress-test suite — scale edit count, file count, vault count, and more to find bottlenecks (measurement only) |
| [optimize.plan.md](optimize.plan.md) | Proposed fixes for the bottlenecks benchmark.plan.md found, one section per dimension, citing measured numbers — draft, pending its own review |
| [cli_refactor.plan.md](cli_refactor.plan.md) | Split src/cli/mod.rs + render.rs into one file per command, fix adapter-import leakage into a single composition-root seam — complete |

## Chapter status

| Chapter | Status | Plan |
|---------|--------|------|
| 0 — Overview & toolchain | In progress | [chapter_0.plan.md](chapter_0.plan.md) |
| 1 — Empty repo, green CI | Complete | [chapter_1.plan.md](chapter_1.plan.md) |
| 2 — Documentation skeleton | Complete | [chapter_2.plan.md](chapter_2.plan.md) |
| 3 — `vault init` + storage | Complete | [chapter_3.plan.md](chapter_3.plan.md) |
| 4 — Singleton watcher | Complete | [chapter_4.plan.md](chapter_4.plan.md) |
| — Ports & adapters refactor | Complete | [architecture.plan.md](architecture.plan.md) |
| 5 — Time-travel commands | Planned | [chapter_5.plan.md](chapter_5.plan.md) |
| 6 — Release v0.1.0 | Pending | [chapter_0.plan.md](chapter_0.plan.md#chapter-6--release-v010) |
| — Benchmark & stress-test suite | Complete | [benchmark.plan.md](benchmark.plan.md) |
| — Optimize bottleneck fixes | Draft, pending review | [optimize.plan.md](optimize.plan.md) |
