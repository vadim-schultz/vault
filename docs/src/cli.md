# CLI reference

Command-line interface for the `vault` binary.

## Global options

| Flag | Description |
|------|-------------|
| `--version` / `-V` | Print version (`vault 0.1.0`) |
| `-v` / `--verbose` | Verbose output (reserved for later chapters) |
| `--vault-path PATH` | Path to the `.vault/` directory (auto-discovered when omitted) |

## Subcommands

| Command | Status | Chapter |
|---------|--------|---------|
| `init` | Implemented | 3 |
| `status` | Implemented | 4 |
| `ignore PATTERN` | Implemented | 4 |
| `show PATH --at DATE` | Stub | 5 |
| `restore PATH --at DATE [--dry-run]` | Stub | 5 |
| `log [PATH]` | Stub | 5 |
| `diff PATH [--at DATE] [--to DATE]` | Stub | 5 |
| `list` | Stub | 5 |

### `vault init`

Initialize a vault in the current directory. Creates `.vault/` with `config.toml`, a recovery
`README`, a bare git object store (`.vault/.git/`), and the `meta.db` SQLite index. Takes a
baseline snapshot of existing files, registers the vault in the global registry, and ensures the
singleton watcher is running.

```bash
vault init
vault init --no-service   # skip daemon install/start (also VAULT_NO_SERVICE=1)
```

Running `vault init` again in the same directory fails with an "already initialized" error.

| Flag | Description |
|------|-------------|
| `--vault-path PATH` | Path to the `.vault/` directory (default: `./.vault` under the current directory) |
| `--no-service` | Do not install or start the background watcher |

### `vault status`

Report daemon health, registered vault count, and the last snapshot time for each vault.

```bash
vault status
```

### `vault ignore`

Append an ignore glob pattern to `.vault/config.toml` when it is not already present.

```bash
vault ignore '*.pdf'
```

### `vault show`

View a file as it was at a given timestamp.

```bash
vault show README.md --at 2026-06-01
vault show design.md --at "2026-06-01 23:58"
```

| Argument / flag | Description |
|-----------------|-------------|
| `PATH` | File path relative to the vault root |
| `--at DATE` | Timestamp (see [Date formats](#date-formats)) |

### `vault restore`

Write an earlier version of a file back to the workspace.

```bash
vault restore README.md --at 2026-06-01
vault restore design.md --at "2026-06-01 23:58" --dry-run
```

| Argument / flag | Description |
|-----------------|-------------|
| `PATH` | File path relative to the vault root |
| `--at DATE` | Timestamp (see [Date formats](#date-formats)) |
| `--dry-run` | Print what would be restored without writing |

### `vault log`

Browse version history for one file or the whole vault.

```bash
vault log
vault log docs/architecture.md
```

### `vault diff`

Compare a file between two points in time.

```bash
vault diff README.md
vault diff README.md --at 2026-06-01 --to 2026-07-01
```

| Flag | Description |
|------|-------------|
| `--at DATE` | Start timestamp (optional) |
| `--to DATE` | End timestamp (optional) |

### `vault list`

List tracked files and their latest version timestamp (Chapter 5).

```bash
vault list
```

## Date formats

MVP accepts explicit timestamps only:

| Format | Meaning |
|--------|---------|
| `YYYY-MM-DD` | Date; start of day UTC |
| `YYYY-MM-DD HH:MM` | Date and time; local timezone |

Relative phrases (`2 weeks ago`, `yesterday`) are deferred to post-v0.1.

## Not exposed to users

| Item | Notes |
|------|-------|
| `vault watch` | Watching starts on `init` via the singleton daemon |
| `vault daemon` | Hidden subcommand; runs the watcher foreground loop (used by systemd and detached spawn) |

## Environment variables

| Variable | Purpose |
|----------|---------|
| `VAULT_STATE_DIR` | Override global state directory (registry, daemon lock, heartbeat) |
| `VAULT_NO_SERVICE` | Skip service install and daemon start on `vault init` |

## Not in v0.1

Multi-machine sync, encryption, retention/prune policies, launchd / Windows Task Scheduler adapters.
