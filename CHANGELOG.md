# Changelog

## Unreleased

### Added

* `vault prune` — removes registry entries whose vault root no longer exists on disk and reports
  which paths were removed (or that there's nothing to prune). Previously that cleanup only ran
  reactively inside the background daemon on registry reload (see below, "`vault status` is
  read-only"), leaving no way to clear stale entries when the daemon wasn't running.

### Changed

* `vault log` now reads like `git log --stat` — a header line (vault's own commit message, no
  commit SHA), an indented diffstat line per changed file, and a totals line, with a blank line
  between commits; unscoped `vault log` now reports which files changed in each snapshot (it
  previously showed nothing but timestamps). `--verbose` swaps the diffstat block for full
  unified-diff hunks per file, like `git log -p`.
* `vault show`'s `PATH` argument is now optional and gains two new scope levels: omitted prints a
  whole-vault report (`git show <rev>` shaped — header + full diff per file, always); a directory
  path scopes that same report to the subtree. An exact file path keeps today's raw-bytes dump,
  byte-for-byte unchanged.
* `vault diff`'s binary-file message now matches git's literal wording
  (`Binary files a/<path> and b/<path> differ`) instead of `Binary files differ.`.
* Commit-message wording for a mixed-kind batch (e.g. one modify + one delete in the same
  snapshot) is now `"change {N} files"` instead of overclaiming `"update {N} files"`.

### Fixed

* Bare-date `--at YYYY-MM-DD` (`show`/`restore`/`diff`) now resolves to the end of that day in the
  host's local timezone instead of UTC start-of-day — previously a bare date never reflected
  anything that happened *on* that date (only the day before), and for a vault's first day could
  fail outright with `no snapshot at or before ...` if the first snapshot landed after that day's
  UTC midnight. `YYYY-MM-DD HH:MM` and RFC3339 forms are unaffected.
* Repo growth no longer unbounded — daemon `git_housekeeping` task repacks when `[gc]` loose-object, pack-file, or max-age thresholds are exceeded; `vault status` shows live counts and last-repack summary.
* Files over `max_file_bytes` no longer skipped silently — `vault status` enumerates currently oversized files under an `oversized (N not tracked)` block; `vault list`/snapshots unaffected (visibility only).
* `resolve_at` (`show`/`diff`/`restore`) and `list_tracked_files` (`vault list`) no longer degrade linearly with total snapshot count — added `idx_snapshots_created_at` and rewrote `SELECT_TRACKED_FILES` to aggregate by path first; legacy `meta.db` files migrate on open.

### Added

* `[gc]` config section — `loose_object_limit` (default 6700), `pack_limit` (50), `max_age_secs` (7 days).
* `git_housekeeping` background task — checks thresholds every 15 minutes per vault; repacks via git2 `PackBuilder` (vendored libgit2, no transport features).
* `.vault/housekeeping.json` marker — last check time, live counts, and last-repack stats.
* `examples/run_housekeeping.rs` — one-shot housekeeping for stress scripts.
* `benches/housekeeping.rs` — `count_objects` and `repack` cost at 100–50k objects.
* `git2` dependency (housekeeping module only; gix remains the read/write object store).

* Background work queue in the daemon — swappable `QueueStore` port, FIFO `InMemoryQueueStore`, `WorkQueue` orchestrator, and a background runner. Long tasks enqueue and return immediately; recurring tasks self-reschedule via `TaskKind::interval`.
* `reconcile_walk` task — periodic safety net (every 10 min per vault) that diffs disk against `list_tracked_files` and logs mismatches to `daemon.log` (partial fix for edit-burst silent data loss; root cause still open).
* `vault status` queue section — reads `queue.json` (written by the daemon heartbeat tick) and lists pending tasks with id, kind, lane, and attempts.
* `benches/queue_latency.rs` — compares synchronous `reconcile_walk` vs `enqueue` cost; results in `.plans/queue/RESULTS.md`.
* Benchmark & stress-test suite (`benches/`, `scripts/stress/`, `examples/simulate_history.rs`) covering history depth, file count, file size, edit burst size, vault count, repo growth with no GC, and reader/writer concurrency — see `.plans/benches/RESULTS.md` for measured knee points and `.plans/benches/optimize.plan.md` for proposed fixes. Manual profiling tools only, not wired into CI.
* Singleton background watcher — one daemon per user watches all registered vaults via `registry.toml` hot reload (`notify`).
* `vault status` — daemon heartbeat, vault count, and last snapshot per registered vault.
* `vault ignore PATTERN` — append ignore globs to `.vault/config.toml`.
* Hidden `vault daemon [--foreground]` — singleton lock, heartbeat writer, and multi-vault watcher loop.
* Global registry (`registry.toml`) with atomic writes under `registry.lock`.
* Snapshot pipeline via gix tree editor + `commit_as` + sqlite transaction; baseline snapshot at `vault init`.
* Systemd user unit adapter (`vault-watcher.service`) with detached-spawn fallback when systemd is unavailable.
* `vault init --no-service` and `VAULT_NO_SERVICE` to skip daemon startup (tests/CI).
* Default ignore pattern `.git/**` for coexistence with foreign git directories at the project root.
* `vault init` — creates `.vault/` with `config.toml`, recovery `README`, gix bare git-dir, and SQLite schema.
* Storage modules: `gix` object store (`src/storage/git.rs`), `rusqlite` metadata index (`src/storage/sqlite/`).
* Integration tests for init layout, re-init guard, config defaults, and schema (`tests/init.rs`).
* Cargo library + binary scaffold with async CLI (`clap`, `tokio`).
* Stub subcommands: `init`, `show`, `restore`, `log`, `diff`, `status`, `list`, `ignore`.
* GitHub Actions CI (`lint-test`, `build-test`, `docs`) and local `scripts/ci.sh` mirror.
* Full mdBook site (getting started, architecture, CLI reference, releasing).
* GitHub Pages deploy workflow (`deploy-docs.yml`).

### Changed

* Drop `InMemoryMetaIndex`, `InMemoryObjectStore`, and `InMemoryRegistry` test fakes; affected unit tests now use real `SqliteMetaIndex`, `GixObjectStore`, and `TomlRegistry` in tempdirs.
* Shape refactor — split oversized modules into focused subdirectories following Sandi Metz sizing rules: `storage/housekeeping/{fs,marker,repack}`, `app/status/model`, `adapters/fakes/*`, `daemon/{guard,heartbeat,queue_snapshot}`, `config/{watcher,gc}`, `storage/git/{worktree_cwd,tree_edit}`, `cli/commands/status/render`. Extracted shared helpers in `queue`, `handlers`, `watcher`, `sqlite`, and `paths` to eliminate duplicated match/lock/path boilerplate. No behavior change.
* Restructured the crate into `domain/`, `ports/`, `adapters/`, and `app/` use-cases with injected trait objects.
* `vault status` is read-only — registry pruning moved to daemon reload (`PruneRegistry` use-case).
* Watcher routing is a single pass; ignore patterns are applied once in the router.
* `--vault-path` no longer implies auto-discovery — run commands from the vault root or pass the path explicitly.
* Path classification moved to `domain::PathKind::classify`, a pure exhaustive match over missing, directory, file, and special paths.
* A debounce batch reuses the vault's compiled `IgnoreMatcher` instead of rebuilding the glob set per batch.

### Fixed

* A filesystem event on a **directory** no longer removes the entire subtree from the git tree. Directory events were classified as deletes, so touching a watched directory silently dropped every tracked file beneath it from the current tree.
* Sockets, fifos, and device nodes are skipped rather than treated as file content.
* Daemon no longer hangs when the watcher task exits with an error.
* Registry hot-reload is reliable across Tokio worker threads (shared reload state instead of thread-local).
* A corrupt vault `config.toml` no longer prevents other registered vaults from being watched.
* `vault status` no longer rewrites `registry.toml` via implicit pruning.
* Git tree paths and SQLite `file_events.path` rows now share the same `RelPath` spelling.
* Cross-vault commits are no longer serialized by a global lock or process-wide `chdir`.
