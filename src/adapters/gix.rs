//! gix-backed [`ObjectStore`] adapter.

use std::sync::Mutex;

use crate::domain::{CommitSha, FileChange, HistoryCommit, RelPath, VaultLayout};
use crate::error::VaultError;
use crate::ports::ObjectStore;
use crate::storage::git::{self, GitStore};

/// Git object store backed by gix.
pub struct GixObjectStore {
    store: Mutex<Option<GitStore>>,
}

impl GixObjectStore {
    /// Open an existing vault git store.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Git`] when the repository cannot be opened.
    pub fn open(layout: &VaultLayout) -> Result<Self, VaultError> {
        let store = git::open(&layout.git_dir_path(), &layout.worktree)?;
        Ok(Self {
            store: Mutex::new(Some(store)),
        })
    }

    /// Initialize a new vault git store.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Git`] when repository creation or open fails.
    pub fn init(layout: &VaultLayout) -> Result<Self, VaultError> {
        let store = git::init(&layout.git_dir_path(), &layout.worktree)?;
        Ok(Self {
            store: Mutex::new(Some(store)),
        })
    }

    fn with_store<T>(
        &self,
        f: impl FnOnce(&GitStore) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let guard = self.store.lock().map_err(|_| VaultError::TaskPanicked)?;
        let store = guard.as_ref().ok_or_else(|| {
            VaultError::git(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "git store not open",
            ))
        })?;
        f(store)
    }
}

impl ObjectStore for GixObjectStore {
    fn commit(
        &self,
        changes: &[FileChange],
        message: &str,
    ) -> Result<Option<CommitSha>, VaultError> {
        self.with_store(|store| {
            store
                .commit_tree(changes, message)
                .map(|opt| opt.map(CommitSha))
        })
    }

    fn read_blob(&self, commit: &CommitSha, path: &RelPath) -> Result<Option<Vec<u8>>, VaultError> {
        self.with_store(|store| store.read_blob_at(commit.as_str(), path))
    }

    fn history(&self) -> Result<Vec<HistoryCommit>, VaultError> {
        self.with_store(GitStore::walk_history)
    }
}
