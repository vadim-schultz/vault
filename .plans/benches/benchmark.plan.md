---
name: Benchmark & Stress-Test Suite
overview: Build a harness that scales Vault along independent load dimensions (edit count, file count, file size, burst size, vault count, history age, reader/writer concurrency) to find where each subsystem degrades. Measurement only — no fixes land here. Two grounded hypotheses already found by reading the code — an unindexed `resolve_at` scan and a full-registry reload per vault — anchor the first two dimensions; the rest are discovered empirically. The final artifact is a separate optimization plan drafted from the measured numbers, reviewed on its own before any fix is implemented.
todos:
  - id: bench-foundation
    content: "benches/ harness scaffolding: criterion dep, fixture generators (synthetic vault trees, scripted edit sequences), a bench-only helper crate/module for driving GitStore/MetaDb/Router directly without the daemon; scripts/stress/ for real end-to-end (daemon + fs) runs"
    status: pending
  - id: bench-dim-history
    content: "History depth (edit count over time): benchmark resolve_at / show / log / diff / restore at 10^2..10^6 snapshots; confirm or refute the unindexed snapshots(created_at) full-scan hypothesis with numbers (no fix applied here)"
    status: pending
  - id: bench-dim-files
    content: "File count (breadth): vault init baseline walk + first commit, and steady-state single-file-edit commit latency, at 10^2..10^6 tracked files; check list/log rendering cost too"
    status: pending
  - id: bench-dim-filesize
    content: "File size: commit latency and memory for files from 1KB to >max_file_bytes; verify the size-limit skip is visible to the user rather than silent"
    status: pending
  - id: bench-dim-burst
    content: "Edit burst within one debounce window: N files touched simultaneously (bulk rename, branch switch, find-replace) at N = 10..100k; single sequential commit_tree cost, event-channel/backlog behavior under OS watch limits"
    status: pending
  - id: bench-dim-vaults
    content: "Vault count per daemon: registry.toml size and Router::from_registry reload cost, plus per-event vault_for lookup, at 10..10k registered vaults"
    status: pending
  - id: bench-dim-object-growth
    content: "Repo growth with no GC: loose-object count/disk usage and read latency over a long simulated history (years of daily edits compressed into one run) — measure the curve, don't design the gc/repack step here"
    status: pending
  - id: bench-dim-concurrency
    content: "Concurrent readers vs. writer: sustained daemon commit load + parallel show/log/diff/status/list, measuring latency percentiles and confirming no SQLITE_BUSY / lock errors surface to the user"
    status: pending
  - id: bench-reporting
    content: "Results doc (per-dimension knee point, metric, verdict: fine / needs limit / needs fix), CHANGELOG + docs update, decide CI wiring (small smoke scale in CI, full scale manual-only)"
    status: pending
  - id: bench-optimize-plan-draft
    content: "Draft optimize.plan.md from the results doc: one section per dimension verdicted 'needs limit' or 'needs fix', each with the measured knee point, the proposed fix or guard, and expected impact — reviewed separately before any implementation starts"
    status: pending
isProject: false
---

# Benchmark & stress-test suite

## Context

The MVP loop (init → forget → come back later → show/diff/restore) works end-to-end today —
verified by hand against a real daemon and a real `.vault/.git` + `meta.db`. The open question
isn't correctness, it's **what happens under load the demo doesn't exercise**: years of daily
edits, a vault with tens of thousands of files, a user who registers a vault per project (hundreds
of vaults on one machine), or a bulk operation that touches thousands of files inside one debounce
window. The goal of this plan is to find the knee point on each of those curves and record it with
hard numbers. **This plan measures only — it does not fix anything.** Deciding whether a finding
gets a graceful limit/warning or an actual fix, and how, is the job of a follow-up optimization
plan (drafted as this plan's final artifact, then reviewed on its own).

**Parent context:** post-MVP hardening — see [mvp/README.md](../mvp/README.md) for the landed
bootstrap summary and [mvp/architecture.md](../mvp/architecture.md) for the architecture reference.

## Dimensions to stress

The user's starting list was edit count, file count, and vault count. Reading the storage and
watcher code surfaces a few more independent axes, each with a concrete hypothesis tied to a real
code path rather than a guess:

| # | Dimension | What actually scales | Where in the code | Hypothesis |
|---|-----------|----------------------|--------------------|------------|
| 1 | **History depth** (total edits over the vault's lifetime) | Rows in `snapshots` / `file_events` | `SELECT_COMMIT_AT_OR_BEFORE` (`src/storage/sqlite/queries.rs:52`) filters `snapshots.created_at` with **no index on that column** — only `idx_file_events_path_time` exists. `resolve_at` backs `show`, `diff`, and `restore`. | Every `show`/`diff`/`restore` call degrades from O(log n) to a full table scan as total snapshot count grows — a vault used daily for a year (~365+ snapshots) probably won't show it, but one watched continuously across thousands of small edits will. This is the most concrete, highest-confidence hypothesis in this plan. |
| 2 | **File count** (breadth) | `WalkDir` traversal at init; per-path `tree.upsert` calls per commit | `walk.rs::collect_baseline_changes` (sequential, one syscall per file); `storage/git.rs::apply_tree_changes` (sequential `upsert_blob_in_tree` per changed path) | Baseline `init` on a 50k-file docs folder walks and commits everything in one shot, synchronously, with no progress feedback — likely the first place a user notices "hanging." Steady-state single-file edits should be cheap regardless of total tracked file count (gix tree editor doesn't rewrite the whole tree) — worth confirming that assumption rather than taking it for granted. |
| 3 | **File size** | Whole-file `std::fs::read` into memory per blob write | `upsert_blob_in_tree` (`src/storage/git.rs:66`) | `max_file_bytes` (default 10MB) bounds this per-file, but changes are silently dropped from the snapshot when a file exceeds it (`walk.rs`, `snapshot.rs` filtering) — worth confirming the user actually sees this rather than wondering why a file "isn't tracked." |
| 4 | **Edit burst size** (files touched inside one debounce window) | One `commit_tree` call processes the whole batch sequentially: N blob writes + N tree edits before the commit lands | `watcher/worker.rs`, `GitStore::commit_tree` | A bulk operation (git branch switch, find-replace across a folder, unzip) that touches thousands of files at once turns into one long blocking commit on the watcher task — and OS-level watch backlogs (`notify`/FSEvents/inotify) may coalesce or drop events before that. |
| 5 | **Vault count** (vaults registered to one daemon) | `registry.toml` is read/parsed/rewritten whole-file on every mutation; `Router::from_registry` reloads **every** vault's config + compiled ignore matcher on every hot-reload tick; `Router::vault_for` is a linear scan per routed path | `registry.rs::load/save`, `watcher/router.rs:49-63,111-116` | Fine at the "a few projects" scale the CLI targets today; someone who runs `vault init` per repo across a large monorepo checkout or dotfiles-per-project setup could plausibly hit dozens-to-hundreds of vaults and start seeing hot-reload lag or slower event routing. |
| 6 | **Repo object growth / no GC** | Loose objects accumulate in `.vault/.git/objects` forever — no `git gc`/repack anywhere in the codebase | `storage/git.rs` (no pack/gc calls); `.plans` and `CHANGELOG.md` confirm no such feature exists yet | Over a long real history this is a disk-usage and (eventually) filesystem-lookup-latency concern (huge fan-out directories under `objects/xx/`), separate from the SQLite query cost in #1. |
| 7 | **Concurrent readers vs. writer** | `meta.db` (WAL mode, `busy_timeout=5000`) and the bare git repo are read by CLI commands while the daemon is mid-commit | `storage/sqlite/queries.rs::CONNECTION_PRAGMAS`; existing test `read_during_write_does_not_busy_error` proves it works at trivial scale | Push concurrency much higher (sustained writer + many parallel readers) to find the actual ceiling before `SQLITE_BUSY` or git lock contention surfaces to a user running `vault show` while a big edit is being committed. |

Stretch, lower priority — only pursue if time remains after the above: directory nesting depth and
path length (tree editor / `RelPath` behavior on deeply nested vaults).

## Decisions baked into this plan (flag before executing if you'd choose differently)

1. **Two-layer harness, not one.** Pure algorithmic cost (dimensions 1, 2, 5) gets Rust
   `criterion` benchmarks calling library code directly (`GitStore`, `MetaDb`, `Router`) — fast,
   precise, no daemon/debounce noise. Dimensions that are inherently about the daemon and real
   filesystem (3, 4, 6, 7) get shell/process-level stress scripts under `scripts/stress/` that spin
   up a real `vault daemon` and drive it with real file edits, matching what a user actually
   experiences (wall-clock, not just CPU cycles).
2. **Fixture generation is synthetic and seeded**, not a real document corpus — deterministic
   file/edit counts matter more than realistic prose for finding a knee point. Content is random
   bytes or repeated boilerplate; only size and count are varied.
3. **No fixes land in this plan.** Every todo ends at "measured and verdicted," never at "and then
   patched." This keeps the numbers honest (nothing here is tuned by a fix made mid-benchmark) and
   keeps the two plans reviewable independently — this one on methodology and findings, the
   optimization plan on proposed changes and risk. The last todo drafts that follow-up plan
   (`optimize.plan.md`) directly from the results doc: one entry per dimension that came back
   "needs limit" or "needs fix," each citing its measured knee point.
4. **Scale ceiling is set per dimension by when it stops being interesting**, not a fixed number —
   stop scaling a dimension once wall-clock crosses roughly 1s for an interactive command (show,
   diff, restore, status, list) or once it's 10x past where real usage would plausibly land,
   whichever comes first. Record the knee point either way.
5. **Not wired into default CI at full scale.** These runs can take minutes and gigabytes of disk.
   A small smoke-scale variant (seconds, megabytes) can run in CI as a regression guard; full-scale
   runs stay a manual `scripts/stress/*.sh` invocation. Exact CI scope is decided in the last todo,
   after we see how slow the full runs actually are.

## Metrics captured per run

- Wall-clock latency (init, per-commit, per-CLI-command), reported as p50/p95/p99 where the
  harness does repeated calls.
- Disk usage: `.vault/.git` size (and loose object count), `.vault/meta.db` size.
- Daemon process RSS and CPU during sustained load (dimensions 4, 6, 7).
- Failure mode at the breaking point: does it get slow-but-correct, error with a clear message, or
  silently drop data / hang / crash? Only the last three count as "not graceful."

## Exit criteria

- Each of the 7 dimensions has a benchmark, a recorded knee point (or "no knee found up to Nx
  realistic scale"), and a verdict: fine as-is / needs a documented limit / needs a fix — landed.
- The two grounded hypotheses (unindexed `resolve_at`, full-registry reload) are confirmed or
  refuted with numbers, not left as speculation.
- A results doc exists summarizing all 7 dimensions in one place for future reference.
- `optimize.plan.md` exists, drafted from that results doc, covering every "needs limit" /
  "needs fix" verdict with its measured knee point — ready for its own review before any
  implementation begins. No code changes to fix a bottleneck happen under this plan.
