//! Git object store via gix (no `git` CLI).

use std::path::{Path, PathBuf};

use gix::create::{into as create_repo, Kind, Options};

use crate::error::VaultError;

/// Handle to the vault's separated git-dir and worktree.
pub struct GitStore {
    repo: gix::Repository,
    git_dir: PathBuf,
    worktree: PathBuf,
}

impl GitStore {
    /// Return the git directory path.
    #[must_use]
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// Return the worktree path.
    #[must_use]
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// Write `data` as a blob and read it back (for tests and validation).
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Git`] when blob I/O fails.
    pub fn write_and_read_blob(&self, data: &[u8]) -> Result<Vec<u8>, VaultError> {
        let oid = self
            .repo
            .write_blob(data)
            .map_err(|e| VaultError::Git(e.to_string()))?;

        let object = self
            .repo
            .find_object(oid)
            .map_err(|e| VaultError::Git(e.to_string()))?;
        let blob = object.into_blob();
        Ok(blob.data.clone())
    }
}

/// Initialize a bare git repository at `git_dir` with external `worktree`.
///
/// The worktree path is recorded for later snapshot operations; the bare
/// git-dir lives only under `.vault/.git/` with no root `.git` file.
///
/// # Errors
///
/// Returns [`VaultError::Git`] when repository creation or open fails.
pub fn init(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    if let Some(parent) = git_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let git_dir = git_dir.to_path_buf();
    let worktree = worktree.to_path_buf();

    with_worktree_cwd(&worktree, || {
        create_repo(&git_dir, Kind::Bare, Options::default())
            .map_err(|e| VaultError::Git(e.to_string()))?;

        let repo = gix::open(&git_dir).map_err(|e| VaultError::Git(e.to_string()))?;

        Ok(GitStore {
            repo,
            git_dir: git_dir.clone(),
            worktree: worktree.clone(),
        })
    })
}

fn with_worktree_cwd<T>(
    worktree: &Path,
    action: impl FnOnce() -> Result<T, VaultError>,
) -> Result<T, VaultError> {
    let restore = std::env::current_dir().ok();
    std::env::set_current_dir(worktree)?;
    let result = action();
    if let Some(dir) = restore {
        let _ = std::env::set_current_dir(dir);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_creates_git_dir_with_objects() {
        let dir = TempDir::new().expect("tempdir");
        let worktree = dir.path().join("project");
        let git_dir = dir.path().join(".vault").join(".git");
        std::fs::create_dir_all(&worktree).expect("worktree");

        let store = init(&git_dir, &worktree).expect("init git");

        assert!(store.git_dir().join("objects").is_dir());
        assert!(store.git_dir().join("HEAD").is_file());
    }

    #[test]
    fn blob_roundtrip() {
        let dir = TempDir::new().expect("tempdir");
        let worktree = dir.path().join("project");
        let git_dir = dir.path().join(".vault").join(".git");
        std::fs::create_dir_all(&worktree).expect("worktree");

        let store = init(&git_dir, &worktree).expect("init git");
        let data = b"hello vault";
        let read_back = store.write_and_read_blob(data).expect("roundtrip");
        assert_eq!(read_back, data);
    }
}
