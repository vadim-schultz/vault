---
name: Chapter 4 Singleton Watcher
overview: One portable singleton daemon watches every registered vault via registry.toml hot-reload. Per-directory .vault/ storage stays portable; global state lives under the user data dir.
todos:
  - id: ch4-deps-tests-red
    content: Add deps (notify, notify-debouncer-full, globset, walkdir, fs4, directories, chrono, gix tree-editor); extend tests/common with vault_env + wait_for
    status: pending
  - id: ch4-registry
    content: Implement registry.rs + tests/registry.rs (atomic registry.toml register/unregister/prune)
    status: pending
  - id: ch4-snapshot
    content: Implement snapshot.rs + baseline walk on init (gix tree editor + commit_as + sqlite tx)
    status: pending
  - id: ch4-watcher
    content: Implement watcher/ (router, worker, debounce) + tests/watcher.rs including hot-reload
    status: pending
  - id: ch4-daemon-service
    content: Implement daemon.rs singleton lock + heartbeat; service/ adapters; hidden vault daemon CLI
    status: pending
  - id: ch4-status-ignore
    content: Implement vault status and vault ignore; wire init registration + ensure_running
    status: pending
  - id: ch4-docs-ci
    content: Update architecture/cli/getting_started docs, CHANGELOG, chapter_0 plan; green ./scripts/ci.sh all
    status: pending
isProject: false
---

# Chapter 4 — Singleton background watcher

## Context

**Prerequisites (merged):** Chapters 1–3 — `vault init` creates `.vault/{README,config.toml,.git,meta.db}`.

**Parent plan:** [chapter_0.plan.md](chapter_0.plan.md) § Chapter 4.

**Design change:** Replaced per-directory `vault-watcher@<path>.service` with **one singleton daemon** per user. `vault init` registers the vault root in global `registry.toml`; the daemon watches that file and hot-reloads its watch set.

## Goal

After `vault init`, versioning runs without user intervention:

| Layer | Responsibility |
|-------|----------------|
| Per directory | Portable `.vault/` (git + sqlite) — unchanged |
| Per user | Singleton daemon + `registry.toml` under state dir |
| Coordination | Daemon watches `registry.toml` via `notify` (portable Linux/macOS/Windows) |

## Exit criteria

| Check | How |
|-------|-----|
| Integration tests green | `cargo test` |
| Multi-vault isolation | Two vaults snapshot into correct `.vault/meta.db` |
| Hot reload | Third vault registered while daemon runs → picked up without restart |
| Singleton | Second `vault daemon --foreground` exits with "already running" |
| `vault status` | Reports daemon state, vault count, last snapshot |
| CI safe | Tests use `VAULT_STATE_DIR` + `VAULT_NO_SERVICE=1`; no `systemctl` |
| No git/sqlite3 CLI | gix + rusqlite only |

## Global state layout

`VAULT_STATE_DIR` overrides default (`~/.local/share/vault/` on Linux):

```text
<state_dir>/
├── registry.toml
├── registry.lock
├── daemon.lock
├── daemon.json
└── daemon.log
```

## TDD order

1. **Red:** `tests/registry.rs`, `tests/watcher.rs`, `tests/daemon.rs`, `tests/status.rs`
2. **Green:** `registry.rs`, `snapshot.rs`, `watcher/`, `daemon.rs`, `service/`, `status.rs`
3. **Refactor + docs**

## Module map

```text
src/registry.rs, ignore.rs, walk.rs, snapshot.rs, daemon.rs, status.rs
src/watcher/{mod.rs, router.rs, worker.rs}
src/service/{mod.rs, systemd.rs, unsupported.rs}
```

## Deferred

- launchd / Windows Task Scheduler adapters (stubs only in v0.1)
- Global cross-vault search
- `vault pause` / `vault resume`

See [chapter_0.plan.md](chapter_0.plan.md) for full architecture diagrams and master-plan edits.
