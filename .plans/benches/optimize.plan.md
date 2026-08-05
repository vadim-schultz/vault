---
name: Optimize — Vault Bottleneck Fixes
overview: Proposed fixes for every dimension RESULTS.md verdicted "needs limit" or "needs fix", each citing its measured knee point from real runs, not speculation. Drafted as the final artifact of the benchmark plan — this plan itself needs its own review before any implementation begins; nothing here is landed yet.
todos:
  - id: opt-burst-investigate-and-fix
    content: "Edit burst silent data loss (dimension 4, highest priority): root-cause why ~16-43% of files vanish above 10k simultaneous creates with zero error (notify/FSEvents coalescing vs. an internal debouncer/channel drop), then either fix the drop or add a detectable failure mode (error, warning, or a periodic reconciliation re-walk) so it's never silent — **partial:** periodic `reconcile_walk` safety net landed via work queue (see `.plans/queue/README.md`); root-cause investigation still open"
    status: pending
  - id: opt-history-index
    content: "History depth (dimension 1): add an index on snapshots(created_at) (or a covering index matching SELECT_COMMIT_AT_OR_BEFORE's ORDER BY), and fix SELECT_TRACKED_FILES's correlated subquery to scale with distinct paths rather than total file_events rows"
    status: completed
  - id: opt-repo-gc
    content: "Repo growth / no GC (dimension 6): add a repack/gc step (on an explicit vault command, a periodic daemon task, or both) to bound the ~10KB-per-commit loose-object overhead measured at 20k commits (200MB for <2MB of real content)"
    status: completed
  - id: opt-filesize-warning
    content: "File size (dimension 3): surface a visible signal (log line, vault status entry, or both) when a file is skipped for exceeding max_file_bytes, instead of the current silent drop"
    status: completed
  - id: opt-vault-count-reload
    content: "Vault count (dimension 5): make Router::from_registry incremental (reload only vaults whose config/ignore actually changed) instead of reloading every registered vault on every hot-reload tick — 129ms at 2,000 vaults today"
    status: pending
  - id: opt-file-count-tree-structure
    content: "File count / flat-tree edit cost (dimension 2): confirm whether nesting tracked files into subdirectories bounds the per-edit tree-rewrite cost (hypothesized but not measured in the benchmark pass); if confirmed, decide between restructuring the git tree strategy vs. documenting a recommended max-files-per-directory limit"
    status: pending
isProject: false
---

# Optimize — Vault bottleneck fixes

## Context

[benchmark.plan.md](benchmark.plan.md) measured 7 load dimensions and recorded every
number in [RESULTS.md](RESULTS.md). Six of the seven came back "needs limit"
or "needs fix"; only concurrent readers-vs-writer (dimension 7) was fine as-is. This plan turns
those six findings into proposed fixes — one section per dimension, each citing its measured
knee point, a concrete proposed change, and how to verify the fix actually moved the number.

**This plan is a draft for review, not an implementation order.** Per the split agreed when
`benchmark.plan.md` was scoped: measurement and fixing are deliberately separate reviews. Nothing
below has been implemented, and none of it should be, until this plan itself is discussed —
same as `benchmark.plan.md` was before its own execution.

## Priority order

Ranked by severity of the failure mode, not by ease of fix:

1. **Edit burst silent data loss** (dimension 4) — the only finding that's an outright correctness
   problem (permanent, silent data loss) rather than a performance curve. Everything else here is
   "gets slow" or "wastes disk"; this one is "loses your files and tells you nothing."
2. **Repo growth / no GC** (dimension 6) — unbounded disk growth (~100x overhead measured) with no
   ceiling; a vault used for years will eventually become a real disk-space problem.
3. **History depth index** (dimension 1) — well-understood root cause (missing index), low-risk
   fix, meaningful payoff at high edit counts.
4. **File size silent skip** (dimension 3) — smallest fix here (surface an existing skip that
   already happens correctly), included because "silent" is the common thread across the top
   three findings and this one is cheap to close.
5. **Vault count reload cost** (dimension 5) — real but only matters at vault counts well past
   what the CLI targets today; lower urgency.
6. **File count / flat-tree edit cost** (dimension 2) — the least understood: the benchmark
   confirmed the cost exists and grows with total tracked files, but didn't confirm the
   subdirectory-sharding hypothesis that would determine whether this needs a structural fix or
   just a documented limit. Investigation before a fix decision.

## 1. Edit burst silent data loss

**Measured:** 0% loss at ≤5,000 simultaneous file creates; 16.4% loss at 10,000; 37.5% at 15,000;
42.8% at 20,000 — reproduced twice, consistent knee point between 5k and 10k. Zero errors or
warnings anywhere (daemon log, `vault status`) when it happens (`RESULTS.md` § 4).

**Proposed fix:** Two-part, since the root cause isn't confirmed yet:

- *Investigate first* — instrument the watcher to log the raw event count `notify` delivers vs.
  the count the debouncer forwards vs. the count that reaches `commit_batch`, across a burst that
  reproduces the loss. This narrows it to one of: (a) the OS backend (FSEvents on macOS,
  inotify/kqueue elsewhere) coalescing or capping events under load, or (b) an internal buffer in
  `notify-debouncer-full` or the watcher's own channel dropping under backpressure.
- *Then fix accordingly*: if (a), the watcher needs to detect the OS-level "rescan needed" signal
  (FSEvents' `kFSEventStreamEventFlagMustScanSubDirs` or equivalent) and fall back to a directory
  walk instead of trusting the itemized event list; if (b), switch to an unbounded or
  backpressure-blocking channel rather than one that drops.
- *Regardless of root cause*, add a safety net: a periodic (or on-`vault status`) reconciliation
  pass that walks tracked watch roots and diffs against `list_tracked_files`, so a future regression
  in this area produces a visible "N files untracked" signal instead of silent loss. This is
  cheap insurance even after the root cause is fixed.

  **Partially landed (2026-08-04):** daemon work queue runs `reconcile_walk` every 10 minutes per
  registered vault and logs mismatches to `daemon.log` — see `.plans/queue/README.md`. Still
  missing: root-cause fix for the burst drop itself, and on-demand reconciliation via
  `vault status` (only daemon log today).

**Verification:** re-run `scripts/stress/edit_burst.sh` at 10k/15k/20k; target 0% loss at all
three, or, if the OS backend itself has a hard ceiling that can't be fully closed, a clearly
surfaced warning at the point loss would otherwise occur.

## 2. Repo growth / no GC

**Measured:** ~10KB of on-disk git object storage per commit for under 100 bytes of actual unique
content — 200MB of loose objects at 20,000 commits to a single file (`RESULTS.md` § 6).
No `git gc`/repack exists anywhere in the codebase today.

**Proposed fix:** Add a repack step using gix's pack-writing support (avoiding a shell-out to the
`git` CLI, consistent with the existing no-CLI-git constraint). Two open questions to resolve
during implementation, not here:

- *Trigger*: an explicit `vault gc` command, a periodic daemon task (e.g. every N commits or every
  M hours), or both. A periodic trigger risks surprising latency spikes on whichever commit
  crosses the threshold; an explicit command risks never being run by a user who doesn't know it
  exists.
- *Scope*: full repack (simplest, but rewrites everything each time — cost grows with total repo
  size) vs. incremental pack-on-top (more moving parts, bounded cost per run).

**Verification:** re-run `scripts/stress/object_growth.sh` before and after a repack point;
confirm `.vault/.git` size after repack is close to the sum of actual unique blob content rather
than ~10KB/commit.

## 3. History depth index

**Measured:** `resolve_at` scales linearly with total snapshot count — 8.5µs at 100 rows, 3.15ms
at 50,000 (`RESULTS.md` § 1). `snapshots` has no index on `created_at`; only
`file_events(path, snapshot_id)` is indexed. Separately, `list_tracked_files` (`vault list`)
scales with total edit count rather than distinct file count because `SELECT_TRACKED_FILES`'s
correlated `MAX(snapshot_id)` subquery is evaluated per underlying row, not per distinct path.

**Proposed fix:**

- Add `CREATE INDEX idx_snapshots_created_at ON snapshots(created_at DESC, id DESC)` (or similar,
  matching `SELECT_COMMIT_AT_OR_BEFORE`'s exact `ORDER BY`) via a schema migration.
- Rewrite `SELECT_TRACKED_FILES` to group by path first (e.g. a window function or a `GROUP BY`
  with `MAX(snapshot_id)` aggregated once per path) rather than a per-row correlated subquery.

**Verification:** re-run `benches/history_depth.rs`; target `resolve_at` and
`list_tracked_files` both closer to flat (or log-scale) across 100 → 50,000 rows instead of the
current clean linear slope.

## 4. File size silent skip

**Measured:** Files over `max_file_bytes` (default 10MB) are correctly excluded from every
snapshot, but nothing — not `vault status`, not a log line, not the exit code of any command —
tells the user this happened (`RESULTS.md` § 3).

**Proposed fix:** Surface the skip somewhere the user will actually see it without asking:
`vault status` gaining a "N files currently over the size limit, not tracked: <paths>" line seems
like the best fit, since it's the command already framed as a health check, and it can enumerate
current oversized files directly from the filesystem walk rather than needing new persistent
state.

**Verification:** manual — drop an oversized file into a watched vault, confirm `vault status`
(or wherever this lands) mentions it.

## 5. Vault count reload cost

**Measured:** `Router::from_registry` — which reloads every registered vault's config and
compiled ignore matcher — takes 129ms at 2,000 vaults, and runs on **every** hot-reload tick
(`RESULTS.md` § 5), pausing the watcher's event processing for that long each time any
vault's registration changes anywhere on the machine.

**Proposed fix:** Cache loaded `WatchedVault`s across reloads and only reload the ones whose
`registered_at`/config file mtime actually changed since the last build, rather than reloading
every entry unconditionally.

**Verification:** re-run `benches/vault_count.rs`'s `router_from_registry` benchmark before/after;
target near-constant cost for a reload that only adds/removes one vault out of many, rather than
linear in total vault count.

## 6. File count / flat-tree edit cost — investigate before fixing

**Measured:** Steady-state single-file commit cost grows with total tracked file count even when
only one file changes — 2.85ms at 1,000 tracked files, 76.8ms at 50,000 (`RESULTS.md`
§ 2) — refuting the initial assumption that gix's tree editor keeps this cheap regardless of
scale. Likely cause: the benchmark fixture is a flat single directory, and a git tree object
serializes its *entire* entry list, so any change re-writes and re-hashes all of it.

**Not proposing a fix yet** — the benchmark didn't test whether nesting into subdirectories
bounds this cost to the size of the deepest single directory (plausible, since each subdirectory
gets its own tree object). That needs its own measurement first:

- Re-run a variant of `benches/file_count.rs`'s steady-state benchmark with the same total file
  count spread across e.g. 100 subdirectories of ~500 files each, and compare against the current
  flat-directory numbers.
- If nesting meaningfully bounds the cost: decide whether to restructure how Vault lays out
  tracked files, or simply document a recommended files-per-directory ceiling for users who
  organize a flat vault.
- If nesting does *not* help (i.e. the cost is inherent to gix's tree editor regardless of
  directory shape): this becomes a harder problem — possibly upstream in gix, possibly requiring
  a different storage strategy than one-git-tree-per-vault-state.

**Verification:** the subdirectory-vs-flat comparison above; only after that data exists does this
todo turn into an actual fix proposal.

## Exit criteria

- Every todo above has a decision recorded (fixed, limited-and-documented, or explicitly deferred
  with a reason) before implementation is considered "this plan is done."
- Every fix implemented has a before/after re-run of its citing benchmark or stress script showing
  the knee point moved, added to `RESULTS.md`.
- Dimension 6 (file count) has its subdirectory-sharding question answered before any structural
  change is attempted there.
