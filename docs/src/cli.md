# CLI reference

Command-line interface for the `vault` binary. Subcommands are **stubs** until the chapters noted
below implement them.

## Global options

| Flag | Description |
|------|-------------|
| `--version` / `-V` | Print version (`vault 0.1.0`) |
| `-v` / `--verbose` | Verbose output (reserved for later chapters) |
| `--vault-path PATH` | Path to the `.vault/` directory (auto-discovered when omitted; Chapter 3+) |

## Subcommands

| Command | Status | Chapter |
|---------|--------|---------|
| `init` | Stub | 3 |
| `show PATH --at DATE` | Stub | 5 |
| `restore PATH --at DATE [--dry-run]` | Stub | 5 |
| `log [PATH]` | Stub | 5 |
| `diff PATH [--at DATE] [--to DATE]` | Stub | 5 |
| `status` | Stub | 4–5 |
| `list` | Stub | 5 |
| `ignore PATTERN` | Stub | 4 |

### `vault init`

Initialize a vault in the current directory. Creates `.vault/`, starts the background watcher
(Chapter 3–4).

```bash
vault init
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

### `vault status`

Report watcher health, last snapshot time, and file count (Chapter 4–5).

```bash
vault status
```

### `vault list`

List tracked files and their latest version timestamp (Chapter 5).

```bash
vault list
```

### `vault ignore`

Add an ignore glob pattern (e.g. `*.pdf`) to `config.toml` (Chapter 4).

```bash
vault ignore '*.pdf'
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
| `vault watch` | Watching starts on `init` via systemd user service |
| `vault internal-watch` | Hidden subcommand for CI and tests (Chapter 4) |

## Not in v0.1

Multi-machine sync, encryption, retention/prune policies, macOS/Windows support.
