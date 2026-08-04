---
name: History Index Optimization
overview: "Implement dimension-1 fixes from the benchmark remediation plan: add a `snapshots(created_at)` index and rewrite `SELECT_TRACKED_FILES` so `resolve_at` and `vault list` stop scaling linearly with total edit count."
todos:
  - id: branch
    content: Sync main, create feat/history-index branch
    status: completed
  - id: tdd-index-red
    content: "Red: init.rs + mod.rs tests for idx_snapshots_created_at and migration on legacy DB open"
    status: completed
  - id: tdd-index-green
    content: "Green: add index to SCHEMA + ensure_schema migration in MetaDb::open"
    status: completed
  - id: tdd-query-red
    content: "Red: unit test — many snapshots, few paths, list_tracked_files returns correct distinct set"
    status: completed
  - id: tdd-query-green
    content: "Green: rewrite SELECT_TRACKED_FILES with GROUP BY subquery"
    status: completed
  - id: verify-bench
    content: Re-run cargo bench --bench history_depth; update RESULTS.md with before/after
    status: completed
  - id: docs-ci
    content: CHANGELOG, .plans/benches/README.md link, ./scripts/ci.sh lint green
    status: completed
isProject: false
---

# History depth — SQLite index & query rewrite

## Why this one first

Reviewed [RESULTS.md](RESULTS.md) and [optimize.plan.md](optimize.plan.md). Six dimensions need work; ranked by **ease of safe implementation today**:

| Fix | Ease | Blocker |
|-----|------|---------|
| **History depth index + query** | **High** | None — root cause confirmed, changes confined to `src/storage/sqlite/` |
| File size warning | High | Touches walk + status across CLI/daemon; behavior-only, no perf proof |
| Vault count incremental reload | Medium | Router caching design decisions |
| Repo GC | Medium | gix pack API + trigger design |
| File count / flat tree | Low | Needs new benchmark variant first |
| Edit burst data loss | Low | Unknown root cause (OS vs channel); highest severity but hardest |

**Chosen:** dimension 1 (history depth). Well-understood, low-risk, additive, and verifiable with the existing [`benches/history_depth.rs`](../../benches/history_depth.rs) harness. Scope includes **both** parts from `opt-history-index`: index on `snapshots.created_at` **and** `SELECT_TRACKED_FILES` rewrite.

**Out of scope for this chapter:** `list_snapshots(None)` (`vault log`) — inherently O(output size) with no pagination; index won't help.

---

## Problem recap (from benchmarks)

| Operation | 100 rows | 50,000 rows | Root cause |
|-----------|----------|-------------|------------|
| `resolve_at` | 8.5 µs | 3.15 ms | No index on `snapshots.created_at` |
| `list_tracked_files` | 31 µs | 15.6 ms | Correlated `MAX(snapshot_id)` per row, not per distinct path |

Bench fixture: 50k snapshots round-robin across **50 paths** — `list` should cost ~50 rows, not 50k.

---

## Implementation (landed)

### A. Index for `resolve_at`

- Added `idx_snapshots_created_at ON snapshots(created_at DESC, id DESC)` to `SCHEMA` for new vaults.
- Added idempotent `ENSURE_SNAPSHOTS_CREATED_AT_INDEX` migration on every `MetaDb::open`.

### B. Rewrite `SELECT_TRACKED_FILES`

Replaced correlated subquery with `GROUP BY path` + join so cost scales with distinct paths.

---

## Verification

Re-ran `cargo bench --bench history_depth` on Linux (release, 2026-08-04). See [RESULTS.md](RESULTS.md) §1 for before/after table.

`list_snapshots(None)` remains linear in output size — expected, not addressed here.

---

## Exit criteria

- [x] `idx_snapshots_created_at` present on fresh `vault init` and migrated on open of legacy `meta.db`
- [x] `resolve_at` and `list_tracked_files` contract tests green
- [x] New regression test for high-edit-count / low-path-count `list_tracked_files`
- [x] `cargo bench --bench history_depth` re-run; RESULTS.md updated with before/after
- [x] CHANGELOG.md entry; README in `.plans/benches/` links this chapter
- [x] `./scripts/ci.sh lint` green
