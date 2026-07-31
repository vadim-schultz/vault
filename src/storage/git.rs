//! Git object store via gix (no `git` CLI).

use std::path::{Path, PathBuf};

use gix::create::{into as create_repo, Kind, Options};
use gix::object::tree::EntryKind;

use crate::error::VaultError;
use crate::snapshot::{FileChange, FileEventKind};

fn rel_path_str(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn git_error(err: impl std::fmt::Display) -> VaultError {
    VaultError::Git(err.to_string())
}

/// Inputs shared by tree-edit handlers.
struct TreeEditContext<'a> {
    repo: &'a gix::Repository,
    worktree: &'a Path,
}

type TreeChangeHandler = fn(
    &TreeEditContext<'_>,
    &mut gix::object::tree::Editor<'_>,
    &FileChange,
) -> Result<(), VaultError>;

fn tree_handler_for(kind: FileEventKind) -> TreeChangeHandler {
    match kind {
        FileEventKind::Create | FileEventKind::Modify => upsert_blob_in_tree,
        FileEventKind::Delete => remove_path_from_tree,
    }
}

fn apply_tree_changes(
    ctx: &TreeEditContext<'_>,
    editor: &mut gix::object::tree::Editor<'_>,
    changes: &[FileChange],
) -> Result<(), VaultError> {
    for change in changes {
        tree_handler_for(change.kind)(ctx, editor, change)?;
    }
    Ok(())
}

fn upsert_blob_in_tree(
    ctx: &TreeEditContext<'_>,
    editor: &mut gix::object::tree::Editor<'_>,
    change: &FileChange,
) -> Result<(), VaultError> {
    let abs = ctx.worktree.join(&change.rel);
    let data = std::fs::read(&abs).map_err(VaultError::Io)?;
    let oid = ctx.repo.write_blob(&data).map_err(git_error)?;
    editor
        .upsert(rel_path_str(&change.rel), EntryKind::Blob, oid)
        .map_err(git_error)?;
    Ok(())
}

fn remove_path_from_tree(
    _ctx: &TreeEditContext<'_>,
    editor: &mut gix::object::tree::Editor<'_>,
    change: &FileChange,
) -> Result<(), VaultError> {
    editor
        .remove(rel_path_str(&change.rel))
        .map_err(git_error)?;
    Ok(())
}

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
        let oid = self.repo.write_blob(data).map_err(git_error)?;

        let object = self.repo.find_object(oid).map_err(git_error)?;
        let blob = object.into_blob();
        Ok(blob.data.clone())
    }

    /// Build a tree from `changes` and create a commit on `HEAD`.
    ///
    /// Returns `None` when the tree would be unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Git`] when tree or commit operations fail.
    pub fn commit_tree(
        &self,
        changes: &[FileChange],
        message: &str,
    ) -> Result<Option<String>, VaultError> {
        with_worktree_cwd(&self.worktree, || self.commit_tree_inner(changes, message))
    }

    fn commit_tree_inner(
        &self,
        changes: &[FileChange],
        message: &str,
    ) -> Result<Option<String>, VaultError> {
        let parent_tree = self.parent_tree_id()?;
        let tree_id = self.build_tree_from_changes(parent_tree, changes)?;
        if tree_id == parent_tree {
            return Ok(None);
        }
        let commit_id = self.create_head_commit(message, tree_id)?;
        Ok(Some(commit_id))
    }

    fn build_tree_from_changes(
        &self,
        parent_tree: gix::ObjectId,
        changes: &[FileChange],
    ) -> Result<gix::ObjectId, VaultError> {
        let mut editor = self.repo.edit_tree(parent_tree).map_err(git_error)?;
        let ctx = TreeEditContext {
            repo: &self.repo,
            worktree: &self.worktree,
        };
        apply_tree_changes(&ctx, &mut editor, changes)?;
        editor.write().map_err(git_error).map(gix::Id::detach)
    }

    fn create_head_commit(
        &self,
        message: &str,
        tree_id: gix::ObjectId,
    ) -> Result<String, VaultError> {
        let parents = self.head_parent_ids();
        let mut time_buf = gix::date::parse::TimeBuf::default();
        let signature = gix::actor::Signature {
            name: "vault".into(),
            email: "vault@localhost".into(),
            time: gix::date::Time::now_utc(),
        };
        let sig = signature.to_ref(&mut time_buf);
        let commit_id = self
            .repo
            .commit_as(sig, sig, "HEAD", message, tree_id, parents)
            .map_err(git_error)?;
        Ok(commit_id.to_string())
    }

    fn head_parent_ids(&self) -> Vec<gix::ObjectId> {
        self.repo
            .head_id()
            .ok()
            .map(|id| vec![id.detach()])
            .unwrap_or_default()
    }

    fn parent_tree_id(&self) -> Result<gix::ObjectId, VaultError> {
        match self.repo.head_commit() {
            Ok(commit) => commit.tree_id().map_err(git_error).map(gix::Id::detach),
            Err(_) => Ok(self.repo.empty_tree().id().detach()),
        }
    }
}

/// Initialize a bare git repository at `git_dir` with external `worktree`.
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
        create_repo(&git_dir, Kind::Bare, Options::default()).map_err(git_error)?;
        open_inner(&git_dir, &worktree)
    })
}

/// Open an existing vault git store.
///
/// # Errors
///
/// Returns [`VaultError::Git`] when the repository cannot be opened.
pub fn open(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    open_inner(git_dir, worktree)
}

fn open_inner(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    with_worktree_cwd(worktree, || {
        let repo = gix::open(git_dir).map_err(git_error)?;
        Ok(GitStore {
            repo,
            git_dir: git_dir.to_path_buf(),
            worktree: worktree.to_path_buf(),
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
