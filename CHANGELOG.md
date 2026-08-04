# Changelog

## Unreleased

### Added

* Benchmark & stress-test suite (`benches/`, `scripts/stress/`, `examples/simulate_history.rs`) covering history depth, file count, file size, edit burst size, vault count, repo growth with no GC, and reader/writer concurrency — see `benches/RESULTS.md` for measured knee points and `.plans/optimize.plan.md` for proposed fixes. Manual profiling tools only, not wired into CI.
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
