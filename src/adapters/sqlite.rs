//! rusqlite-backed [`MetaIndex`] adapter.

use std::path::Path;

use crate::domain::{CommitSha, RelPath, SnapshotEntry, SnapshotRecord, TrackedFile};
use crate::domain::FileEventKind;
use crate::error::VaultError;
use crate::ports::MetaIndex;
use crate::storage::sqlite::MetaDb;

/// `SQLite` metadata index with a held connection.
pub struct SqliteMetaIndex {
    db: MetaDb,
}

impl SqliteMetaIndex {
    /// Open or create `meta.db`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the database cannot be created or opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VaultError> {
        Ok(Self {
            db: MetaDb::open(path.as_ref())?,
        })
    }
}

impl MetaIndex for SqliteMetaIndex {
    fn record_snapshot(&self, record: &SnapshotRecord) -> Result<(), VaultError> {
        self.db.insert_snapshot(record)
    }

    fn last_snapshot_time(&self) -> Result<Option<String>, VaultError> {
        self.db.last_snapshot_time()
    }

    fn resolve_at(&self, at: &str) -> Result<Option<CommitSha>, VaultError> {
        Ok(self.db.resolve_at(at)?.map(CommitSha))
    }

    fn list_snapshots(&self, path: Option<&RelPath>) -> Result<Vec<SnapshotEntry>, VaultError> {
        self.db
            .list_snapshots(path.map(RelPath::as_str))?
            .into_iter()
            .map(to_snapshot_entry)
            .collect()
    }

    fn list_tracked_files(&self) -> Result<Vec<TrackedFile>, VaultError> {
        self.db.list_tracked_files()?.into_iter().map(to_tracked_file).collect()
    }
}

fn to_snapshot_entry(row: (String, String, Option<String>)) -> Result<SnapshotEntry, VaultError> {
    let (sha, created_at, event) = row;
    Ok(SnapshotEntry {
        commit_sha: CommitSha(sha),
        created_at,
        event: parse_event(event)?,
    })
}

fn parse_event(event: Option<String>) -> Result<Option<FileEventKind>, VaultError> {
    event
        .map(|e| {
            FileEventKind::parse(&e).ok_or_else(|| VaultError::CorruptMetaIndex {
                detail: format!("unknown event_type {e:?}"),
            })
        })
        .transpose()
}

fn to_tracked_file(row: (String, String)) -> Result<TrackedFile, VaultError> {
    let (path, last_modified) = row;
    Ok(TrackedFile {
        path: RelPath::parse(&path),
        last_modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::meta_index::contract;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    #[test]
    fn resolve_at_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::resolve_at_returns_latest_commit_at_or_before(index);
    }

    #[test]
    fn list_snapshots_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::list_snapshots_filters_and_orders(index);
    }

    #[test]
    fn list_tracked_files_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::list_tracked_files_excludes_deleted(index);
    }
}
