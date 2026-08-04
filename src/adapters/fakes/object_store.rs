//! In-memory object store fake.

use std::sync::Mutex;

use crate::domain::{CommitSha, FileChange, RelPath};
use crate::error::VaultError;
use crate::ports::ObjectStore;

/// In-memory object store.
pub struct InMemoryObjectStore {
    commits: Mutex<Vec<(CommitSha, Vec<FileChange>)>>,
}

impl Default for InMemoryObjectStore {
    fn default() -> Self {
        Self {
            commits: Mutex::new(Vec::new()),
        }
    }
}

impl ObjectStore for InMemoryObjectStore {
    fn commit(
        &self,
        changes: &[FileChange],
        _message: &str,
    ) -> Result<Option<CommitSha>, VaultError> {
        let sha = CommitSha(format!(
            "fake-{}",
            self.commits
                .lock()
                .map_err(|_| VaultError::TaskPanicked)?
                .len()
        ));
        self.commits
            .lock()
            .map_err(|_| VaultError::TaskPanicked)?
            .push((sha.clone(), changes.to_vec()));
        Ok(Some(sha))
    }

    fn read_blob(
        &self,
        _commit: &CommitSha,
        _path: &RelPath,
    ) -> Result<Option<Vec<u8>>, VaultError> {
        Ok(None)
    }
}
