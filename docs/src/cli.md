# CLI reference

Command-line interface for the `vault` binary.

## Global options

| Flag | Description |
|------|-------------|
| `--version` / `-V` | Print version (`vault 0.1.0`) |
| `-v` / `--verbose` | For `vault log`: show full diff hunks per file instead of a diffstat (`git log -p` vs. `git log --stat`) |
| `--vault-path PATH` | Path to the `.vault/` directory (default: `./.vault` under the current directory) |

## Subcommands

| Command | Status | Chapter |
|---------|--------|---------|
| `init` | Implemented | 3 |
| `status` | Implemented | 4 |
| `ignore PATTERN` | Implemented | 4 |
| `show [PATH] --at DATE` | Implemented | 5 |
| `restore PATH --at DATE [--dry-run]` | Implemented | 5 |
| `log [PATH]` | Implemented | 5 |
| `diff PATH [--at DATE] [--to DATE]` | Implemented | 5 |
| `list` | Implemented | 5 |

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

`PATH` is optional and selects one of three scope levels, resolved against every path the vault
has ever tracked (so a since-deleted file still resolves as a file, not a directory prefix):

| `PATH` | Output |
|--------|--------|
| An exact file path (`README.md`) | That file's raw bytes at `--at`, unchanged — the only form worth piping (`vault show README.md --at ... > old.md`) |
| A directory prefix (`docs/`) | A report for that subtree: header line + full unified diff per file touched by the resolved commit, like `git show <rev> -- docs/` |
| Omitted | The same report for the whole vault, like plain `git show <rev>` |

The report forms always print full diffs — there's no `--verbose` gate, since `show` is already
pinned to one resolved commit (unlike `log`, which walks potentially many and stays terse by
default; see below).

```bash
vault show README.md --at 2026-06-01
vault show design.md --at "2026-06-01 23:58"
vault show docs --at 2026-06-01       # directory report
vault show --at 2026-06-01            # whole-vault report
```

| Argument / flag | Description |
|-----------------|-------------|
| `PATH` | File or directory path relative to the vault root; omit for the whole vault |
| `--at DATE` | Timestamp (see [Date formats](#date-formats)) |

### `vault restore`

Write an earlier version of a file back to the workspace. This immediately records its own
snapshot — tagged `restore` in `vault log`, distinct from an organic edit — rather than waiting
for the background watcher to notice the write. Restoring to the version that's already current
is a no-op: nothing is written and no new snapshot is created.

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

Browse version history for one file or the whole vault, `git log --stat` shaped: a header line
(vault's own commit message — no commit SHA), then an indented diffstat line per file changed,
then a totals line, with a blank line between commits.

```bash
vault log
vault log docs/architecture.md
```

```
update notes.md @ 2026-08-05T12:58:27.962477+00:00
 notes.md | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

restore notes.md @ 2026-08-05T12:58:15.669883+00:00
 notes.md | 1 +
 1 file changed, 1 insertion(+)
```

`--verbose` swaps the diffstat lines for full unified-diff hunks per file, like `git log -p`:

```bash
vault --verbose log notes.md
```

`vault log`'s scope filters which commits are listed and which files' diffstat lines are shown,
but each commit's header always reflects everything that commit touched (matching real
`git log --stat -- PATH`) — a commit that changed two files still shows its own two-file message
even when `log` is scoped to just one of them.

Power users can inspect the vault's real git history directly — `.vault/` wraps an ordinary git
repository (`VaultLayout::git_dir_path`, `.vault/.git`) — with `git --git-dir=.vault/.git log --stat`,
which (per the above) now looks nearly identical to `vault log`'s own output, modulo the hash line.

### `vault diff`

Compare a file between two points in time.

| Flags given | Compares |
|-------------|----------|
| neither | last snapshot vs. the working tree |
| `--at` only | that snapshot vs. the working tree |
| `--at` and `--to` | one snapshot vs. another |

`--to` without `--at` is a usage error (there's no natural "diff to" without a start point).

```bash
vault diff README.md
vault diff README.md --at 2026-06-01
vault diff README.md --at 2026-06-01 --to 2026-07-01
```

| Flag | Description |
|------|-------------|
| `--at DATE` | Start timestamp (optional) |
| `--to DATE` | End timestamp (optional; requires `--at`) |

### `vault list`

List tracked files and their latest version timestamp. Files whose most recent event is a
delete are excluded.

```bash
vault list
```

## Date formats

MVP accepts explicit timestamps only:

| Format | Meaning |
|--------|---------|
| `YYYY-MM-DD` | Date; end of day, local timezone |
| `YYYY-MM-DD HH:MM` | Date and time; local timezone |
| RFC3339 (e.g. `2026-06-01T14:32:01+00:00`) | Exact timestamp, any offset |

A bare date is inclusive of that day's activity — `--at 2026-06-01` shows the latest snapshot
taken on or before the end of June 1st, so same-day edits show up without needing to name the next
day. `vault log` prints exact RFC3339 timestamps, so its output round-trips directly back into
`--at`/`--to` — copy a line from `vault log` straight into `vault show --at`.

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
