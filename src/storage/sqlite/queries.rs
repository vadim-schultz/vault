//! SQL statements for the vault metadata index (`meta.db`).

/// Schema applied on `vault init`.
pub const SCHEMA: &str = "
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
CREATE INDEX idx_file_events_path_time ON file_events(path, snapshot_id);
";

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
