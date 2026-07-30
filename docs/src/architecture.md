# Architecture

Vault is a **Linux-first CLI** for automatic version history. Users run `vault init` once;
a background watcher records every change. Later, `vault show` and `vault restore` resolve file
content at a wall-clock timestamp.

## High-level flow

```mermaid
flowchart TB
    subgraph dayOne [Day one]
        Init["vault init"]
    end
    subgraph ongoing [Ongoing - automatic]
        Watcher[Background watcher]
        Snap[Snapshot on change]
    end
    subgraph later [Weeks later]
        Show["vault show --at DATE"]
        Restore["vault restore --at DATE"]
    end

    Init --> Watcher
    Docs[Your documents] -->|save| Watcher
    Watcher --> Snap
    Snap --> VaultStore[.vault/]
    Show --> VaultStore
    Restore --> VaultStore
```

```mermaid
flowchart LR
    subgraph workspace [Workspace]
        Docs[Tracked docs]
    end
    subgraph vaultDir [.vault/]
        Config[config.toml]
        GitDir[.git/]
        MetaDB[meta.db]
        Readme[README]
    end
    Watcher[Background watcher]
    CLI[vault show/log/restore]

    Docs -->|inotify events| Watcher
    Watcher -->|gix commit| GitDir
    Watcher -->|rusqlite insert| MetaDB
    CLI -->|resolve --at date| MetaDB
    CLI -->|gix read blob| GitDir
```

## `.vault/` layout

After `vault init` (Chapter 3), each vault root contains:

```text
.vault/
├── README              # Plain-English layout guide (no tool required)
├── config.toml         # Scope, ignore globs, watcher settings
├── .git/               # Standard Git dir (objects + refs = source of truth)
└── meta.db             # SQLite index for fast time-based queries
```

| Store | Holds | Why |
|-------|-------|-----|
| **Git** (`.git/`) | Blob content, commits, trees | Standard git object store; written/read via **gix** in Rust — no `git` CLI dependency |
| **SQLite** (`meta.db`) | File path → commit SHA, wall-clock timestamp, event type | Fast "latest version before DATE" without scanning full git history |
| **config.toml** | Watched roots, ignore patterns, vault metadata | Sensible defaults; rarely edited |

Vault uses a **separated git-dir** inside `.vault/.git/`. The work-tree is the vault root.
Vault never writes a `.git` file at the project root, so it can coexist with an existing
source-control repository.

## Internal implementation (not user-facing)

All git operations live in `src/storage/git.rs` using **gix**. Vault never shells out to the
`git` binary. On each snapshot:

- A gix commit records changed blobs with messages like `vault: update docs/arch.md @ 2026-07-29T14:32:01Z`
- A row is inserted into SQLite linking paths, commit SHA, and timestamps

The background watcher (Chapter 4) uses `notify` (inotify on Linux) with debouncing. Watching
starts automatically on `init` via a **systemd user service** — users do not run a manual daemon.

See the [CLI reference](cli.md) for user-facing commands.

## Inspect without `vault`

Normal users never need this section. Artifacts are portable by design so power users and
forensics can recover data without the tool.

### `.vault/README`

Created at `vault init` (Chapter 3). Plain-English description of the layout and recovery steps.

### Git CLI (optional)

Because `.vault/.git/` is a standard object store, you can inspect history with the system `git`
binary:

```bash
git --git-dir=.vault/.git log --oneline
git --git-dir=.vault/.git show HEAD:README.md
```

Vault does not invoke `git` internally, but the on-disk layout is compatible.

### SQLite (optional)

Inspect the time index:

```bash
sqlite3 .vault/meta.db ".schema"
```

Schema created at init (Chapter 3):

```sql
CREATE TABLE snapshots (
    id INTEGER PRIMARY KEY,
    commit_sha TEXT NOT NULL,
    created_at TEXT NOT NULL  -- ISO-8601 UTC
);
CREATE TABLE file_events (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER REFERENCES snapshots(id),
    path TEXT NOT NULL,
    event_type TEXT NOT NULL,  -- create | modify | delete
    UNIQUE(snapshot_id, path)
);
CREATE INDEX idx_file_events_path_time ON file_events(path, snapshot_id);
```

Example query (after snapshots exist):

```sql
SELECT s.created_at, f.path, f.event_type
FROM file_events f
JOIN snapshots s ON f.snapshot_id = s.id
ORDER BY s.created_at DESC
LIMIT 20;
```

### Coexistence with project Git

Vault stores git metadata only under `.vault/.git/`. Your project's own `.git/` at the repo root
is untouched.
