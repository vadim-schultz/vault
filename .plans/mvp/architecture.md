# Vault layered architecture

Ports-and-adapters layout for the vault crate — domain at the center, use-cases orchestrate ports,
adapters implement I/O. Full module tree and control-flow diagrams lived in the bootstrap chapter
plans; this file is the durable reference for anyone extending the codebase.

## Dependency rule

Dependencies point **inward**:

| Layer | May import | Must not import |
|-------|------------|-----------------|
| `domain/` | std only | ports, adapters, app, cli, daemon, watcher |
| `ports/` | `domain`, `error` | adapters, app, cli |
| `app/` | `domain`, `ports`, `error` | adapters (concrete types) |
| `adapters/` | `domain`, `ports`, `error` | app, cli |
| `cli/`, `daemon/`, `watcher/` | `app`, `domain`, `ports`, `error` | adapters (except composition root) |
| `bin/vault.rs` | everything | — (composition root) |

## Module tree

```text
src/
├── lib.rs, error.rs
├── domain/          # RelPath, FileChange, SnapshotRecord, VaultLayout, VaultState
├── ports/           # ObjectStore, MetaIndex, RegistryStore, ServiceManager, Clock
├── adapters/        # gix, sqlite, toml_registry, systemd, fakes (test)
├── app/             # InitVault, SnapshotVault, StatusQuery, PruneRegistry, AddIgnore
├── cli/
│   ├── mod.rs       # Cli/Command, dispatch — marshalling only
│   ├── context.rs   # Stores::open() — composition-root seam for concrete adapters
│   ├── support.rs   # run_blocking(), rel_path_from_cli(), Global
│   └── commands/    # one file per subcommand (Args + run + render)
├── daemon.rs        # singleton lock, heartbeat, run_foreground
├── watcher/         # router, worker
├── config.rs, ignore.rs, walk.rs, paths.rs
└── bin/vault.rs     # composition root
```

## Port catalogue

| Port | Role | Production adapter | Test fake |
|------|------|-------------------|-----------|
| `ObjectStore` | commit trees, read blobs at a commit | `GixObjectStore` | real `GixObjectStore` in tempdir |
| `MetaIndex` | record snapshots, resolve `--at` dates | `SqliteMetaIndex` | real `SqliteMetaIndex` in tempdir |
| `RegistryStore` | global `registry.toml` | `TomlRegistry` | real `TomlRegistry` + `VAULT_STATE_DIR` |
| `ServiceManager` | start singleton daemon | `SystemdService` / `DetachedSpawnService` | `RecordingServiceManager` |
| `Clock` | injectable wall clock | `SystemClock` | `FixedClock` |

Use-cases receive `Arc<dyn Port>` — no generics at call sites. `adapters/fakes/` holds only
`FixedClock` and `RecordingServiceManager`; storage and registry ports are tested against
production adapters in tempdirs, behind `#[cfg(any(test, feature = "testing"))]`.

## Composition root

`bin/vault.rs` (via `cli::context::Stores::open` and `cli::context::clock`) wires concrete
adapters. `cli/context.rs` is the **only** file under `cli/` allowed to name concrete adapter types.

## Key invariants

1. **One path spelling** — `RelPath` is the only normalization; git trees and sqlite rows use `RelPath::as_str()`.
2. **No process-global CWD** — gix opens with an absolute git-dir; CWD is restored after commit operations.
3. **Read-only status** — `StatusQuery` never mutates `registry.toml`; pruning is `PruneRegistry` on daemon reload.
4. **No path auto-discovery** — commands run from the vault root or pass `--vault-path` explicitly.
5. **Ignore once** — `Router` applies `IgnoreMatcher`; downstream code trusts pre-filtered `RelPath` lists.

## Storage layout

```text
.vault/
├── README, config.toml
├── .git/            # bare git object store (gix, no git CLI)
└── meta.db          # SQLite time index

<state_dir>/         # per-user singleton daemon state
├── registry.toml, registry.lock
├── daemon.lock, daemon.json, daemon.log
```
