//! SQL statements for the vault metadata index (`meta.db`).

/// Connection pragmas applied on every open.
pub const CONNECTION_PRAGMAS: &str =
    "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;";

/// Return 1 when the `snapshots` table exists (schema already applied).
pub const COUNT_SNAPSHOTS_TABLE: &str =
    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'snapshots'";

/// Return 1 when an index with the given name exists.
pub const COUNT_INDEX_BY_NAME: &str =
    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1";

/// Index matching ``file_events(path, snapshot_id)``.
pub const IDX_FILE_EVENTS_PATH_TIME: &str = "idx_file_events_path_time";

/// Index matching ``snapshots(created_at DESC, id DESC)``.
pub const IDX_SNAPSHOTS_CREATED_AT: &str = "idx_snapshots_created_at";

macro_rules! schema_tables {
    () => {
        "
CREATE TABLE snapshots (
    id INTEGER PRIMARY KEY,
    commit_sha TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE file_events (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER REFERENCES snapshots(id),
    path TEXT NOT NULL,
    event_type TEXT NOT NULL,
    UNIQUE(snapshot_id, path)
);
"
    };
}

/// Schema as applied before ``idx_snapshots_created_at`` existed.
#[cfg(test)]
pub const LEGACY_SCHEMA: &str = concat!(
    schema_tables!(),
    "CREATE INDEX idx_file_events_path_time ON file_events(path, snapshot_id);
",
);

/// Schema applied on `vault init`.
pub const SCHEMA: &str = concat!(
    schema_tables!(),
    "CREATE INDEX idx_file_events_path_time ON file_events(path, snapshot_id);
",
    "CREATE INDEX idx_snapshots_created_at ON snapshots(created_at DESC, id DESC);
",
);

/// Idempotent migration applied on every `meta.db` open for vaults created before the index existed.
pub const ENSURE_SNAPSHOTS_CREATED_AT_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_snapshots_created_at ON snapshots(created_at DESC, id DESC);";

/// Insert one snapshot row.
pub const INSERT_SNAPSHOT: &str = "INSERT INTO snapshots (commit_sha, created_at) VALUES (?1, ?2)";

/// Insert one file event row for a snapshot.
pub const INSERT_FILE_EVENT: &str =
    "INSERT INTO file_events (snapshot_id, path, event_type) VALUES (?1, ?2, ?3)";

/// Count all snapshots.
pub const COUNT_SNAPSHOTS: &str = "SELECT COUNT(*) FROM snapshots";

/// Latest snapshot timestamp by row id.
pub const SELECT_LAST_SNAPSHOT_TIME: &str =
    "SELECT created_at FROM snapshots ORDER BY id DESC LIMIT 1";

/// Most recent event type for a path across snapshots.
pub const SELECT_LATEST_EVENT_TYPE: &str = "
SELECT f.event_type FROM file_events f
JOIN snapshots s ON f.snapshot_id = s.id
WHERE f.path = ?1
ORDER BY s.id DESC
LIMIT 1
";

/// Latest commit SHA at or before a timestamp.
pub const SELECT_COMMIT_AT_OR_BEFORE: &str = "
SELECT commit_sha FROM snapshots
WHERE created_at <= ?1
ORDER BY created_at DESC, id DESC
LIMIT 1
";

/// All snapshots, newest first.
pub const SELECT_ALL_SNAPSHOTS: &str =
    "SELECT commit_sha, created_at FROM snapshots ORDER BY created_at DESC, id DESC";

/// Snapshots that touched a specific path, with that path's event type, newest first.
pub const SELECT_SNAPSHOTS_FOR_PATH: &str = "
SELECT s.commit_sha, s.created_at, f.event_type
FROM file_events f
JOIN snapshots s ON f.snapshot_id = s.id
WHERE f.path = ?1
ORDER BY s.created_at DESC, s.id DESC
";

/// Every path touched by a commit, with its event type, ordered by path.
pub const SELECT_CHANGESET_FOR_COMMIT: &str = "
SELECT f.path, f.event_type
FROM file_events f
JOIN snapshots s ON f.snapshot_id = s.id
WHERE s.commit_sha = ?1
ORDER BY f.path
";

/// The commit that most recently touched `path` before the snapshot for `commit_sha`.
pub const SELECT_PREVIOUS_COMMIT_FOR_PATH: &str = "
SELECT s2.commit_sha
FROM file_events f
JOIN snapshots s2 ON f.snapshot_id = s2.id
WHERE f.path = ?1
  AND f.snapshot_id < (SELECT id FROM snapshots WHERE commit_sha = ?2)
ORDER BY f.snapshot_id DESC
LIMIT 1
";

/// Every distinct path ever recorded, regardless of delete state, ordered by path.
pub const SELECT_ALL_PATHS: &str = "SELECT DISTINCT path FROM file_events ORDER BY path";

/// The recorded `created_at` for a specific commit.
pub const SELECT_CREATED_AT_FOR_COMMIT: &str =
    "SELECT created_at FROM snapshots WHERE commit_sha = ?1";

/// Latest non-delete event per path, ordered by path.
pub const SELECT_TRACKED_FILES: &str = "
SELECT f.path, s.created_at
FROM file_events f
JOIN snapshots s ON f.snapshot_id = s.id
JOIN (
    SELECT path, MAX(snapshot_id) AS snapshot_id
    FROM file_events
    GROUP BY path
) latest ON f.path = latest.path AND f.snapshot_id = latest.snapshot_id
WHERE f.event_type != 'delete'
ORDER BY f.path
";
