//! rusqlite-backed [`MetaIndex`] adapter.

use std::path::Path;

use crate::domain::{CommitSha, SnapshotRecord};
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
}
