//! `SQLite` metadata index for time-based queries.

mod queries;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::domain::SnapshotRecord;
use crate::error::VaultError;

pub use queries::SCHEMA;

/// `SQLite` metadata index with a held connection.
pub struct MetaDb {
    conn: Mutex<Connection>,
}

impl MetaDb {
    /// Open or create `meta.db` and apply schema when missing.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the database cannot be created or initialized.
    pub fn open(path: &Path) -> Result<Self, VaultError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(queries::CONNECTION_PRAGMAS)?;
        let table_count: i64 =
            conn.query_row(queries::COUNT_SNAPSHOTS_TABLE, [], |row| row.get(0))?;
        if table_count == 0 {
            conn.execute_batch(queries::SCHEMA)?;
        }
        conn.execute(queries::ENSURE_SNAPSHOTS_CREATED_AT_INDEX, [])?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert a snapshot row and associated file events in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the insert fails.
    pub fn insert_snapshot(&self, record: &SnapshotRecord) -> Result<(), VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            queries::INSERT_SNAPSHOT,
            params![record.commit_sha.as_str(), record.created_at],
        )?;
        let snapshot_id = tx.last_insert_rowid();
        for change in &record.changes {
            tx.execute(
                queries::INSERT_FILE_EVENT,
                params![snapshot_id, change.rel.as_str(), change.kind.as_str()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Return the number of snapshots.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the query fails.
    pub fn snapshot_count(&self) -> Result<i64, VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let count: i64 = conn.query_row(queries::COUNT_SNAPSHOTS, [], |row| row.get(0))?;
        Ok(count)
    }

    /// Return the latest snapshot timestamp, if any.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the query fails.
    pub fn last_snapshot_time(&self) -> Result<Option<String>, VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let result = conn.query_row(queries::SELECT_LAST_SNAPSHOT_TIME, [], |row| {
            row.get::<_, String>(0)
        });
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Return file events for the latest snapshot matching `path`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the query fails.
    pub fn latest_event_type(&self, file_path: &str) -> Result<Option<String>, VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let result = conn.query_row(
            queries::SELECT_LATEST_EVENT_TYPE,
            params![file_path],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Return the latest commit SHA at or before `at`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the query fails.
    pub fn resolve_at(&self, at: &str) -> Result<Option<String>, VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let result = conn.query_row(queries::SELECT_COMMIT_AT_OR_BEFORE, params![at], |row| {
            row.get::<_, String>(0)
        });
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// List snapshots, optionally scoped to a path, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the query fails.
    pub fn list_snapshots(
        &self,
        path: Option<&str>,
    ) -> Result<Vec<(String, String, Option<String>)>, VaultError> {
        match path {
            Some(path) => self.list_snapshots_for_path(path),
            None => self.list_all_snapshots(),
        }
    }

    fn list_all_snapshots(&self) -> Result<Vec<(String, String, Option<String>)>, VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let mut stmt = conn.prepare(queries::SELECT_ALL_SNAPSHOTS)?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, None)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn list_snapshots_for_path(
        &self,
        path: &str,
    ) -> Result<Vec<(String, String, Option<String>)>, VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let mut stmt = conn.prepare(queries::SELECT_SNAPSHOTS_FOR_PATH)?;
        let rows = stmt.query_map(params![path], |row| {
            Ok((row.get(0)?, row.get(1)?, Some(row.get(2)?)))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List tracked files whose latest event is not a delete.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the query fails.
    pub fn list_tracked_files(&self) -> Result<Vec<(String, String)>, VaultError> {
        let conn = self.conn.lock().map_err(|_| VaultError::TaskPanicked)?;
        let mut stmt = conn.prepare(queries::SELECT_TRACKED_FILES)?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// Create `meta.db` and apply the vault schema.
///
/// # Errors
///
/// Returns [`VaultError`] when the database cannot be created or initialized.
pub fn init_meta_db(path: &Path) -> Result<(), VaultError> {
    MetaDb::open(path)?;
    Ok(())
}

/// Insert a snapshot row and associated file events.
///
/// # Errors
///
/// Returns [`VaultError`] when the insert fails.
pub fn insert_snapshot(path: &Path, record: &SnapshotRecord) -> Result<(), VaultError> {
    MetaDb::open(path)?.insert_snapshot(record)
}

/// Return the number of snapshots in `meta.db`.
///
/// # Errors
///
/// Returns [`VaultError`] when the query fails.
pub fn snapshot_count(path: &Path) -> Result<i64, VaultError> {
    MetaDb::open(path)?.snapshot_count()
}

/// Return the latest snapshot timestamp, if any.
///
/// # Errors
///
/// Returns [`VaultError`] when the query fails.
pub fn last_snapshot_time(path: &Path) -> Result<Option<String>, VaultError> {
    MetaDb::open(path)?.last_snapshot_time()
}

/// Return the most recent event type for `file_path`.
///
/// # Errors
///
/// Returns [`VaultError`] when the query fails.
pub fn latest_event_type(path: &Path, file_path: &str) -> Result<Option<String>, VaultError> {
    MetaDb::open(path)?.latest_event_type(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CommitSha, FileChange, FileEventKind, RelPath, SnapshotRecord};
    use tempfile::NamedTempFile;

    #[test]
    fn schema_creates_expected_tables() {
        let file = NamedTempFile::new().expect("tempfile");
        let db = MetaDb::open(file.path()).expect("init");

        let conn = db.conn.lock().expect("lock");
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();

        assert_eq!(tables, vec!["file_events", "snapshots"]);
    }

    #[test]
    fn insert_snapshot_roundtrip() {
        let file = NamedTempFile::new().expect("tempfile");
        let db = MetaDb::open(file.path()).expect("init");
        let changes = vec![FileChange {
            rel: RelPath::parse("notes.md"),
            kind: FileEventKind::Create,
        }];
        let record = SnapshotRecord {
            commit_sha: CommitSha("abc123".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            changes,
        };
        db.insert_snapshot(&record).expect("insert");
        assert_eq!(db.snapshot_count().expect("count"), 1);
        assert_eq!(
            db.last_snapshot_time().expect("time"),
            Some("2026-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn resolve_at_returns_latest_commit_at_or_before() {
        let file = NamedTempFile::new().expect("tempfile");
        let db = MetaDb::open(file.path()).expect("init");
        for (sha, at) in [
            ("aaa", "2026-01-01T10:00:00Z"),
            ("bbb", "2026-01-02T10:00:00Z"),
        ] {
            let record = SnapshotRecord {
                commit_sha: CommitSha(sha.to_string()),
                created_at: at.to_string(),
                changes: vec![],
            };
            db.insert_snapshot(&record).expect("insert");
        }
        assert_eq!(
            db.resolve_at("2026-01-01T12:00:00Z").expect("resolve"),
            Some("aaa".to_string())
        );
        assert_eq!(
            db.resolve_at("2026-01-03T00:00:00Z").expect("resolve"),
            Some("bbb".to_string())
        );
    }

    #[test]
    fn read_during_write_does_not_busy_error() {
        use std::sync::Arc;
        use std::thread;

        let file = NamedTempFile::new().expect("tempfile");
        let db = Arc::new(MetaDb::open(file.path()).expect("init"));
        let reader = Arc::clone(&db);
        let handle = thread::spawn(move || {
            for _ in 0..20 {
                let _ = reader.snapshot_count();
                thread::yield_now();
            }
        });
        for i in 0..20 {
            let record = SnapshotRecord {
                commit_sha: CommitSha(format!("sha{i}")),
                created_at: format!("2026-01-01T{i:02}:00:00Z"),
                changes: vec![],
            };
            db.insert_snapshot(&record).expect("insert");
        }
        handle.join().expect("join");
    }

    #[test]
    fn schema_creates_snapshots_created_at_index() {
        let file = NamedTempFile::new().expect("tempfile");
        let db = MetaDb::open(file.path()).expect("init");

        let conn = db.conn.lock().expect("lock");
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_snapshots_created_at'",
                [],
                |row| row.get(0),
            )
            .expect("query index");
        assert_eq!(index_count, 1);
    }

    #[test]
    fn open_migrates_legacy_schema_without_snapshots_index() {
        let file = NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(file.path()).expect("open legacy db");
            conn.execute_batch(queries::CONNECTION_PRAGMAS)
                .expect("pragmas");
            conn.execute_batch(
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
CREATE INDEX idx_file_events_path_time ON file_events(path, snapshot_id);
",
            )
            .expect("legacy schema");
        }

        let db = MetaDb::open(file.path()).expect("migrate on open");
        let conn = db.conn.lock().expect("lock");
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_snapshots_created_at'",
                [],
                |row| row.get(0),
            )
            .expect("query index");
        assert_eq!(index_count, 1);
    }

    #[test]
    fn list_tracked_files_returns_latest_per_path_with_many_edits() {
        let file = NamedTempFile::new().expect("tempfile");
        let db = MetaDb::open(file.path()).expect("init");
        db.insert_snapshot(&SnapshotRecord {
            commit_sha: CommitSha("snap0".to_string()),
            created_at: "2026-01-01T10:00:00Z".to_string(),
            changes: vec![
                FileChange {
                    rel: RelPath::parse("a.md"),
                    kind: FileEventKind::Create,
                },
                FileChange {
                    rel: RelPath::parse("b.md"),
                    kind: FileEventKind::Create,
                },
            ],
        })
        .expect("insert baseline");

        for i in 1..=200 {
            db.insert_snapshot(&SnapshotRecord {
                commit_sha: CommitSha(format!("snap{i}")),
                created_at: format!("2026-01-01T{i:02}:00:00Z"),
                changes: vec![FileChange {
                    rel: RelPath::parse("a.md"),
                    kind: FileEventKind::Modify,
                }],
            })
            .expect("insert modify");
        }

        let tracked = db.list_tracked_files().expect("list tracked");
        assert_eq!(tracked.len(), 2);
        assert_eq!(tracked[0].0, "a.md");
        assert_eq!(tracked[0].1, "2026-01-01T200:00:00Z");
        assert_eq!(tracked[1].0, "b.md");
        assert_eq!(tracked[1].1, "2026-01-01T10:00:00Z");
    }
}
