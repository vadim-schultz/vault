#![allow(clippy::missing_errors_doc)]

//! Git object store port.

use crate::domain::{CommitSha, FileChange, RelPath};
use crate::error::VaultError;

/// Git object store — commit trees, read blobs at a commit.
pub trait ObjectStore: Send + Sync {
    /// Commit `changes` and return the new commit SHA, if the tree changed.
    fn commit(
        &self,
        changes: &[FileChange],
        message: &str,
    ) -> Result<Option<CommitSha>, VaultError>;

    /// Read blob content at `commit` for `path`.
    fn read_blob(&self, commit: &CommitSha, path: &RelPath) -> Result<Option<Vec<u8>>, VaultError>;
}
