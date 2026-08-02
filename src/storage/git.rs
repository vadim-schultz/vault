//! Git object store via gix (no `git` CLI).

use std::path::{Path, PathBuf};

use gix::create::{into as create_repo, Kind, Options};
use gix::object::tree::EntryKind;

use crate::domain::{FileChange, FileEventKind};
use crate::error::VaultError;

fn with_fallback_cwd<T>(
    fallback: &Path,
    action: impl FnOnce() -> Result<T, VaultError>,
) -> Result<T, VaultError> {
    let restore = if let Ok(current) = std::env::current_dir() {
        Some(current)
    } else {
        std::env::set_current_dir(fallback)?;
        None
    };
    let result = action();
    if let Some(dir) = restore {
        let _ = std::env::set_current_dir(dir);
    }
    result
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
        FileEventKind::Create | FileEventKind::Modify | FileEventKind::Restore => {
            upsert_blob_in_tree
        }
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
    let abs = ctx.worktree.join(change.rel.to_path());
    let data = std::fs::read(&abs).map_err(VaultError::Io)?;
    let oid = ctx.repo.write_blob(&data).map_err(VaultError::git)?;
    editor
        .upsert(change.rel.as_str(), EntryKind::Blob, oid)
        .map_err(VaultError::git)?;
    Ok(())
}

fn remove_path_from_tree(
    _ctx: &TreeEditContext<'_>,
    editor: &mut gix::object::tree::Editor<'_>,
    change: &FileChange,
) -> Result<(), VaultError> {
    editor
        .remove(change.rel.as_str())
        .map_err(VaultError::git)?;
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
        let restore = std::env::current_dir().ok();
        let result = self.commit_tree_inner(changes, message);
        if let Some(dir) = restore {
            let _ = std::env::set_current_dir(dir);
        }
        result
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
        let mut editor = self.repo.edit_tree(parent_tree).map_err(VaultError::git)?;
        let ctx = TreeEditContext {
            repo: &self.repo,
            worktree: &self.worktree,
        };
        apply_tree_changes(&ctx, &mut editor, changes)?;
        editor.write().map_err(VaultError::git).map(gix::Id::detach)
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
            .map_err(VaultError::git)?;
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
            Ok(commit) => commit
                .tree_id()
                .map_err(VaultError::git)
                .map(gix::Id::detach),
            Err(_) => Ok(self.repo.empty_tree().id().detach()),
        }
    }

    /// Read blob content for `path` as it existed in `commit_sha`.
    ///
    /// Returns `None` when the commit does not exist or the path is absent from its tree.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Git`] when the object database cannot be read.
    pub fn read_blob_at(
        &self,
        commit_sha: &str,
        path: &crate::domain::RelPath,
    ) -> Result<Option<Vec<u8>>, VaultError> {
        let Some(commit_id) = parse_commit_id(commit_sha) else {
            return Ok(None);
        };
        let Some(tree) = self.find_commit_tree(commit_id)? else {
            return Ok(None);
        };
        Self::read_entry(&tree, path)
    }
}

fn parse_commit_id(commit_sha: &str) -> Option<gix::ObjectId> {
    gix::ObjectId::from_hex(commit_sha.as_bytes()).ok()
}

impl GitStore {
    fn find_commit_tree(
        &self,
        commit_id: gix::ObjectId,
    ) -> Result<Option<gix::Tree<'_>>, VaultError> {
        match self.repo.find_commit(commit_id) {
            Ok(commit) => commit.tree().map(Some).map_err(VaultError::git),
            Err(_) => Ok(None),
        }
    }

    fn read_entry(
        tree: &gix::Tree<'_>,
        path: &crate::domain::RelPath,
    ) -> Result<Option<Vec<u8>>, VaultError> {
        let Some(entry) = tree
            .lookup_entry_by_path(path.as_str())
            .map_err(VaultError::git)?
        else {
            return Ok(None);
        };
        let object = entry.object().map_err(VaultError::git)?;
        Ok(Some(object.data.clone()))
    }
}

/// Initialize a bare git repository at `git_dir` with external `worktree`.
///
/// # Errors
///
/// Returns [`VaultError::Git`] when repository creation or open fails.
pub fn init(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    with_fallback_cwd(worktree, || {
        if let Some(parent) = git_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        create_repo(git_dir, Kind::Bare, Options::default()).map_err(VaultError::git)?;
        open(git_dir, worktree)
    })
}

/// Open an existing vault git store.
///
/// # Errors
///
/// Returns [`VaultError::Git`] when the repository cannot be opened.
pub fn open(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    with_fallback_cwd(worktree, || {
        let repo = gix::open(git_dir).map_err(VaultError::git)?;
        Ok(GitStore {
            repo,
            git_dir: git_dir.to_path_buf(),
            worktree: worktree.to_path_buf(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RelPath;
    use crate::domain::{FileChange, FileEventKind};
    use tempfile::TempDir;

    fn init_store(dir: &TempDir) -> GitStore {
        let worktree = dir.path().join("project");
        let git_dir = dir.path().join(".vault").join(".git");
        std::fs::create_dir_all(&worktree).expect("worktree");
        init(&git_dir, &worktree).expect("init")
    }

    #[test]
    fn init_creates_git_dir_with_objects() {
        let dir = TempDir::new().expect("tempdir");
        let store = init_store(&dir);
        assert!(store.git_dir().join("objects").is_dir());
        assert!(store.git_dir().join("HEAD").is_file());
    }

    #[test]
    fn commit_succeeds_when_cwd_is_elsewhere() {
        let dir = TempDir::new().expect("tempdir");
        let other = TempDir::new().expect("other");
        let restore = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(other.path()).expect("chdir");

        let store = init_store(&dir);
        std::fs::write(store.worktree().join("b.md"), b"b").expect("write");
        let changes = vec![FileChange {
            rel: RelPath::parse("b.md"),
            kind: FileEventKind::Create,
        }];
        let sha = store
            .commit_tree(&changes, "test")
            .expect("commit")
            .expect("sha");
        assert!(!sha.is_empty());
        std::env::set_current_dir(restore).expect("restore");
    }

    #[test]
    fn read_blob_returns_content_at_commit() {
        let dir = TempDir::new().expect("tempdir");
        let store = init_store(&dir);

        std::fs::write(store.worktree().join("a.md"), b"v1").expect("write");
        let changes1 = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        let sha1 = store
            .commit_tree(&changes1, "commit 1")
            .expect("commit")
            .expect("sha");

        std::fs::write(store.worktree().join("a.md"), b"v2").expect("write");
        let changes2 = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Modify,
        }];
        let sha2 = store
            .commit_tree(&changes2, "commit 2")
            .expect("commit")
            .expect("sha");

        assert_eq!(
            store
                .read_blob_at(&sha1, &RelPath::parse("a.md"))
                .expect("read1"),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            store
                .read_blob_at(&sha2, &RelPath::parse("a.md"))
                .expect("read2"),
            Some(b"v2".to_vec())
        );
    }

    #[test]
    fn read_blob_returns_none_for_untracked_path() {
        let dir = TempDir::new().expect("tempdir");
        let store = init_store(&dir);
        std::fs::write(store.worktree().join("a.md"), b"a").expect("write");
        let changes = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        let sha = store
            .commit_tree(&changes, "test")
            .expect("commit")
            .expect("sha");

        assert_eq!(
            store
                .read_blob_at(&sha, &RelPath::parse("missing.md"))
                .expect("read"),
            None
        );
    }

    #[test]
    fn read_blob_returns_none_for_unknown_commit() {
        let dir = TempDir::new().expect("tempdir");
        let store = init_store(&dir);
        let fake_sha = "0".repeat(40);

        assert_eq!(
            store
                .read_blob_at(&fake_sha, &RelPath::parse("a.md"))
                .expect("read"),
            None
        );
    }
}
