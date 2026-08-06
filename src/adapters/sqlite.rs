//! rusqlite-backed [`MetaIndex`] adapter.

use std::path::Path;

use crate::domain::FileEventKind;
use crate::domain::{CommitSha, FileChange, RelPath, SnapshotEntry, SnapshotRecord, TrackedFile};
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
        Ok(self
            .db
            .list_tracked_files()?
            .into_iter()
            .map(to_tracked_file)
            .collect())
    }

    fn changeset(&self, commit_sha: &CommitSha) -> Result<Vec<FileChange>, VaultError> {
        self.db
            .changeset(commit_sha.as_str())?
            .into_iter()
            .map(to_file_change)
            .collect()
    }

    fn previous_commit_for(
        &self,
        path: &RelPath,
        commit_sha: &CommitSha,
    ) -> Result<Option<CommitSha>, VaultError> {
        Ok(self
            .db
            .previous_commit_for(path.as_str(), commit_sha.as_str())?
            .map(CommitSha))
    }

    fn all_paths(&self) -> Result<Vec<RelPath>, VaultError> {
        Ok(self
            .db
            .all_paths()?
            .into_iter()
            .map(|p| RelPath::parse(&p))
            .collect())
    }

    fn created_at_for(&self, commit_sha: &CommitSha) -> Result<Option<String>, VaultError> {
        self.db.created_at_for(commit_sha.as_str())
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

fn to_file_change(row: (String, String)) -> Result<FileChange, VaultError> {
    let (path, event_type) = row;
    Ok(FileChange {
        rel: RelPath::parse(&path),
        kind: FileEventKind::parse(&event_type).ok_or_else(|| VaultError::CorruptMetaIndex {
            detail: format!("unknown event_type {event_type:?}"),
        })?,
    })
}

fn to_tracked_file(row: (String, String)) -> TrackedFile {
    let (path, last_modified) = row;
    TrackedFile {
        path: RelPath::parse(&path),
        last_modified,
    }
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

    #[test]
    fn changeset_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::changeset_returns_every_path_touched_by_a_commit(index);
    }

    #[test]
    fn changeset_for_unknown_commit_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::changeset_for_unknown_commit_is_empty(index);
    }

    #[test]
    fn previous_commit_for_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::previous_commit_for_finds_prior_touch(index);
    }

    #[test]
    fn all_paths_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::all_paths_includes_deleted(index);
    }

    #[test]
    fn created_at_for_contract() {
        let file = NamedTempFile::new().expect("tempfile");
        let index = Arc::new(SqliteMetaIndex::open(file.path()).expect("open"));
        contract::created_at_for_returns_recorded_timestamp(index);
    }
}
