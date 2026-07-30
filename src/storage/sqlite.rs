//! `SQLite` metadata index for time-based queries.

use std::path::Path;

use rusqlite::Connection;

use crate::error::VaultError;

/// SQL schema applied on `vault init`.
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
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
