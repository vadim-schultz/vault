# MVP — bootstrap summary

Vault reached a working MVP: `vault init` once, background snapshots via a singleton daemon, and
time-travel read commands (`show`, `log`, `diff`, `restore`, `list`, `status`, `ignore`). The
detailed chapter-by-chapter bootstrap plans are archived under [`chapters/`](chapters/) for
historical reference; this page is the living summary.

## Vision

Automatic document versioning without git knowledge. Run `vault init` in a docs directory, edit
files normally, then weeks later:

```bash
vault show README.md --at 2026-06-01
vault restore design.md --at "2026-06-01 23:58"
```

**Init once, forget** — watching starts on init. **Portable artifacts** — `.vault/` uses standard
`.git/` layout, SQLite, and TOML; inspectable without the tool.

## Implementation stages

| Stage | Status | What landed |
|-------|--------|-------------|
| 0 — Toolchain & overview | Done | Rust stable, project vision, tech stack |
| 1 — Empty repo, green CI | Done | Cargo scaffold, `vault --version`, `scripts/ci.sh`, GitHub Actions |
| 2 — Documentation skeleton | Done | mdBook site, architecture + CLI doc stubs |
| 3 — `vault init` + storage | Done | `.vault/` layout, gix object store, SQLite schema, baseline commit |
| 4 — Singleton watcher | Done | `registry.toml`, debounced notify watcher, daemon, `status`/`ignore` |
| — Ports & adapters refactor | Done | Layered crate layout ([architecture.md](architecture.md)) |
| — CLI module split | Done | `cli/commands/*`, `Stores::open` composition root ([chapters/cli_refactor.plan.md](chapters/cli_refactor.plan.md)) |
| 5 — Time-travel commands | Done | `show`/`log`/`diff`/`restore`/`list`, `--at` date parsing, smoke tests |
| — Showcase script | Done | `scripts/showcase.sh` — narrated walkthrough with git/sqlite inspection |
| 6 — Release v0.1.0 | Pending | GitHub Release binary, crates.io, release checklist |

## Architecture reference

See [architecture.md](architecture.md) for the ports-and-adapters layout, module tree, port
catalogue, and invariants. User-facing architecture docs live in the mdBook site (`docs/src/`).

## Archived chapter plans

| Plan | Topic |
|------|-------|
| [chapter_0.plan.md](chapters/chapter_0.plan.md) | Master bootstrap overview (vision, `.vault/` layout, CLI design) |
| [chapter_1.plan.md](chapters/chapter_1.plan.md) | Cargo scaffold, minimal CLI, CI |
| [chapter_2.plan.md](chapters/chapter_2.plan.md) | mdBook site |
| [chapter_3.plan.md](chapters/chapter_3.plan.md) | `vault init`, storage foundation |
| [chapter_4.plan.md](chapters/chapter_4.plan.md) | Singleton daemon + watcher |
| [chapter_5.plan.md](chapters/chapter_5.plan.md) | Time-travel read commands |
| [cli_refactor.plan.md](chapters/cli_refactor.plan.md) | Per-command CLI split |
| [showcase_script.plan.md](chapters/showcase_script.plan.md) | `scripts/showcase.sh` demo |

## Next stream

Post-MVP work on scale and bottlenecks lives under [`.plans/benches/`](../benches/) — measurement
(`benchmark.plan.md`, `RESULTS.md`) and proposed fixes (`optimize.plan.md`).
