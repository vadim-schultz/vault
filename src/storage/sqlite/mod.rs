//! `SQLite` metadata index for time-based queries.

mod queries;

use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::VaultError;
use crate::snapshot::FileChange;

pub use queries::SCHEMA;

/// Create `meta.db` and apply the vault schema.
///
/// # Errors
///
/// Returns [`VaultError`] when the database cannot be created or initialized.
pub fn init_meta_db(path: &Path) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)?;
    conn.execute_batch(queries::SCHEMA)?;
    Ok(())
}

/// Insert a snapshot row and associated file events in one transaction.
///
/// # Errors
///
/// Returns [`VaultError`] when the insert fails.
pub fn insert_snapshot(
    path: &Path,
    commit_sha: &str,
    created_at: &str,
    changes: &[FileChange],
) -> Result<(), VaultError> {
    let conn = Connection::open(path)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(queries::INSERT_SNAPSHOT, params![commit_sha, created_at])?;
    let snapshot_id = tx.last_insert_rowid();
    for change in changes {
        tx.execute(
            queries::INSERT_FILE_EVENT,
            params![
                snapshot_id,
                change.rel.display().to_string(),
                change.kind.as_str()
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Return the number of snapshots in `meta.db`.
///
/// # Errors
///
/// Returns [`VaultError`] when the query fails.
pub fn snapshot_count(path: &Path) -> Result<i64, VaultError> {
    let conn = Connection::open(path)?;
    let count: i64 = conn.query_row(queries::COUNT_SNAPSHOTS, [], |row| row.get(0))?;
    Ok(count)
}

/// Return the latest snapshot timestamp, if any.
///
/// # Errors
///
/// Returns [`VaultError`] when the query fails.
pub fn last_snapshot_time(path: &Path) -> Result<Option<String>, VaultError> {
    let conn = Connection::open(path)?;
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
pub fn latest_event_type(path: &Path, file_path: &str) -> Result<Option<String>, VaultError> {
    let conn = Connection::open(path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{FileChange, FileEventKind};
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn schema_creates_expected_tables() {
        let file = NamedTempFile::new().expect("tempfile");
        init_meta_db(file.path()).expect("init");

        let conn = Connection::open(file.path()).expect("open");
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();

        assert_eq!(tables, vec!["file_events", "snapshots"]);

        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_file_events_path_time'",
                [],
                |row| row.get(0),
            )
            .expect("index count");
        assert_eq!(index_count, 1);
    }

    #[test]
    fn insert_snapshot_roundtrip() {
        let file = NamedTempFile::new().expect("tempfile");
        init_meta_db(file.path()).expect("init");
        let changes = vec![FileChange {
            rel: PathBuf::from("notes.md"),
            kind: FileEventKind::Create,
        }];
        insert_snapshot(file.path(), "abc123", "2026-01-01T00:00:00Z", &changes).expect("insert");
        assert_eq!(snapshot_count(file.path()).expect("count"), 1);
        assert_eq!(
            last_snapshot_time(file.path()).expect("time"),
            Some("2026-01-01T00:00:00Z".to_string())
        );
    }
}
