---
name: Git Housekeeping
overview: "Implement dimension-6 fix: conditional git repack when loose-object count, pack count, or time-since-last-repack thresholds are exceeded — background queue task, vault status surfacing, git2 PackBuilder repack scoped to housekeeping only."
todos:
  - id: branch
    content: Sync main, create feat/git-housekeeping branch
    status: completed
  - id: config
    content: Add GcConfig (loose_object_limit, pack_limit, max_age_secs) to VaultConfig with serde defaults
    status: completed
  - id: housekeeping-core-red-green
    content: "TDD: count_objects + is_due pure functions in src/storage/housekeeping.rs"
    status: completed
  - id: add-git2-dep
    content: Add git2 with vendored-libgit2 only; confirm local build
    status: completed
  - id: repack-red-green
    content: "TDD: repack() via git2 PackBuilder; integration test proving gix reads packed blobs"
    status: completed
  - id: maybe-run-marker
    content: maybe_run orchestration + .vault/housekeeping.json marker read/write
    status: completed
  - id: queue-taskkind
    content: Add TaskKind::GitHousekeeping with constructors + interval/name/vault_root arms
    status: completed
  - id: queue-handler
    content: Wire handlers::git_housekeeping dispatch + daemon.log on actual repack
    status: completed
  - id: daemon-seed
    content: Seed git_housekeeping recurring task per vault at daemon startup
    status: completed
  - id: status-integration
    content: Surface housekeeping in VaultStatus and CLI Display
    status: completed
  - id: bench-housekeeping
    content: Add benches/housekeeping.rs (count_objects + repack cost at 100..50k objects)
    status: completed
  - id: stress-before-after
    content: Add examples/run_housekeeping.rs; extend scripts/stress/object_growth.sh
    status: completed
  - id: showcase-section
    content: "Add scripts/showcase.sh section 14 for live housekeeping demo"
    status: completed
  - id: docs-changelog
    content: CHANGELOG, optimize.plan.md opt-repo-gc completed, .plans/queue/README.md
    status: completed
  - id: ci-green
    content: ./scripts/ci.sh lint green before push
    status: completed
isProject: false
---

# Repo growth — conditional git housekeeping

## Problem recap (from benchmarks)

| Total commits | Loose objects | `.vault/.git` size |
|---------------|--------------:|-------------------:|
| 100 | 300 | 1.26 MB |
| 20,000 | 50,000 | 200 MB |

~10 KB disk per commit for under 100 bytes of unique content, with no repack — see [RESULTS.md](RESULTS.md) § 6.

## Implementation (landed)

- **`[gc]` config** in `.vault/config.toml` — `loose_object_limit` (6700), `pack_limit` (50), `max_age_secs` (7 days).
- **`src/storage/housekeeping.rs`** — `count_objects`, `is_due`, `repack` (git2 `PackBuilder` only here), `maybe_run`, `.vault/housekeeping.json` marker.
- **`TaskKind::GitHousekeeping`** — recurring every 15 minutes; seeded at daemon startup alongside `reconcile_walk`.
- **`vault status`** — live loose/pack counts + last-repack summary per vault.

## Verification

- Unit/integration tests in `housekeeping.rs` and `handlers.rs`.
- `cargo bench --bench housekeeping` for count/repack cost curve.
- `bash scripts/stress/object_growth.sh` records before/after housekeeping columns.
- `scripts/showcase.sh` section 14 demonstrates threshold crossing + repack.

## Exit criteria

- [x] Repack runs when any threshold is exceeded; no-op otherwise.
- [x] gix `GitStore` reads blobs after git2 repack (interop test).
- [x] `opt-repo-gc` marked completed in [optimize.plan.md](optimize.plan.md).
- [x] `./scripts/ci.sh lint` green.
