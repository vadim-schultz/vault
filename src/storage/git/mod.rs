//! Git object store via gix (no `git` CLI).

mod tree_diff;
mod tree_edit;
mod worktree_cwd;

use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;
use gix::create::{into as create_repo, Kind, Options};

use crate::domain::{CommitSha, FileChange, HistoryCommit};
use crate::error::VaultError;

use tree_edit::{apply_tree_changes, TreeEditContext};
use worktree_cwd::WorktreeCwd;

type GitStoreHandler = fn(&Path, &Path) -> Result<GitStore, VaultError>;

fn dispatch_in_worktree(
    git_dir: &Path,
    worktree: &Path,
    handler: GitStoreHandler,
) -> Result<GitStore, VaultError> {
    let _cwd = WorktreeCwd::enter(worktree)?;
    handler(git_dir, worktree)
}

fn create_bare_store(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    if let Some(parent) = git_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    create_repo(git_dir, Kind::Bare, Options::default()).map_err(VaultError::git)?;
    open_existing_store(git_dir, worktree)
}

fn open_existing_store(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    let repo = gix::open(git_dir).map_err(VaultError::git)?;
    Ok(GitStore {
        repo,
        git_dir: git_dir.to_path_buf(),
        worktree: worktree.to_path_buf(),
    })
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
        let _cwd = WorktreeCwd::enter(&self.worktree)?;
        self.commit_tree_inner(changes, message)
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

    /// Walk this store's commit history from `HEAD`, oldest first, with each commit's changes
    /// diffed against its parent's tree (or the empty tree, for the root commit).
    ///
    /// Vault's own commits are always single-parent — `head_parent_ids` above never attaches
    /// more than one — so this refuses with [`VaultError::NonLinearHistory`] rather than
    /// silently picking a parent when a commit has more than one, since that would hide history
    /// a manually-mutated `.vault/.git` might contain. A repository with no commits yet
    /// (headless) returns an empty list.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Git`] when a commit, its message, or its tree cannot be read, and
    /// [`VaultError::NonLinearHistory`] when a commit has more than one parent.
    pub fn walk_history(&self) -> Result<Vec<HistoryCommit>, VaultError> {
        let mut commits = self.collect_commits_from_head()?;
        commits.reverse();
        self.diff_consecutive_commits(&commits)
    }

    /// Walk the parent chain from `HEAD`, newest first. Empty for a headless repo.
    fn collect_commits_from_head(&self) -> Result<Vec<WalkedCommit<'_>>, VaultError> {
        let Ok(head_id) = self.repo.head_id() else {
            return Ok(Vec::new());
        };
        let mut commits = Vec::new();
        let mut current = Some(head_id.detach());
        while let Some(id) = current {
            let commit = self.walk_one_commit(id)?;
            current = commit.parent;
            commits.push(commit);
        }
        Ok(commits)
    }

    /// Read one commit's message, committer time, tree, and single parent.
    fn walk_one_commit(&self, id: gix::ObjectId) -> Result<WalkedCommit<'_>, VaultError> {
        let commit = self.repo.find_commit(id).map_err(VaultError::git)?;
        Ok(WalkedCommit {
            id,
            parent: single_parent(&commit, id)?,
            message: commit_message(&commit)?,
            committer_time: committer_time_rfc3339(&commit)?,
            tree: commit.tree().map_err(VaultError::git)?,
        })
    }

    /// Diff each commit's tree against the one before it (or the empty tree, for the first).
    fn diff_consecutive_commits(
        &self,
        commits: &[WalkedCommit<'_>],
    ) -> Result<Vec<HistoryCommit>, VaultError> {
        let empty = self.repo.empty_tree();
        commits
            .iter()
            .enumerate()
            .map(|(i, commit)| {
                let parent_tree = if i == 0 { &empty } else { &commits[i - 1].tree };
                to_history_commit(commit, parent_tree)
            })
            .collect()
    }
}

/// One commit read directly off `.git`, before it's turned into a [`HistoryCommit`].
struct WalkedCommit<'repo> {
    id: gix::ObjectId,
    message: String,
    committer_time: String,
    tree: gix::Tree<'repo>,
    parent: Option<gix::ObjectId>,
}

/// `commit`'s one parent, or `None` for a root commit.
///
/// # Errors
///
/// Returns [`VaultError::NonLinearHistory`] when `commit` has more than one parent.
fn single_parent(
    commit: &gix::Commit<'_>,
    id: gix::ObjectId,
) -> Result<Option<gix::ObjectId>, VaultError> {
    let mut parents = commit.parent_ids();
    let parent = parents.next().map(gix::Id::detach);
    if parents.next().is_some() {
        return Err(VaultError::NonLinearHistory {
            commit_sha: id.to_string(),
        });
    }
    Ok(parent)
}

/// `commit`'s raw message as UTF-8.
fn commit_message(commit: &gix::Commit<'_>) -> Result<String, VaultError> {
    let raw = commit.message_raw().map_err(VaultError::git)?;
    raw.to_str().map_err(VaultError::git).map(str::to_string)
}

/// Diff `commit` against `parent_tree` and assemble the [`HistoryCommit`] `walk_history` returns.
fn to_history_commit(
    commit: &WalkedCommit<'_>,
    parent_tree: &gix::Tree<'_>,
) -> Result<HistoryCommit, VaultError> {
    let changes = tree_diff::diff_trees(parent_tree, &commit.tree)?;
    Ok(HistoryCommit {
        sha: CommitSha(commit.id.to_string()),
        message: commit.message.clone(),
        changes,
        committer_time: commit.committer_time.clone(),
    })
}

fn parse_commit_id(commit_sha: &str) -> Option<gix::ObjectId> {
    gix::ObjectId::from_hex(commit_sha.as_bytes()).ok()
}

/// `commit`'s own committer timestamp, formatted the same way `Clock::now().to_rfc3339()` does
/// elsewhere in the crate. Only used by `walk_history` as a fallback for a message that doesn't
/// parse — it's read at a different, later instant than the `Clock`-sourced `created_at` a live
/// `meta.db` would have recorded (see `create_head_commit`), so it's lower fidelity, not wrong.
fn committer_time_rfc3339(commit: &gix::Commit<'_>) -> Result<String, VaultError> {
    let time = commit.time().map_err(VaultError::git)?;
    chrono::DateTime::from_timestamp(time.seconds, 0)
        .ok_or_else(|| {
            VaultError::git(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "commit {} has an out-of-range committer timestamp",
                    commit.id()
                ),
            ))
        })
        .map(|dt| dt.to_rfc3339())
}

/// Initialize a bare git repository at `git_dir` with external `worktree`.
///
/// # Errors
///
/// Returns [`VaultError::Git`] when repository creation or open fails.
pub fn init(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    dispatch_in_worktree(git_dir, worktree, create_bare_store)
}

/// Open an existing vault git store.
///
/// # Errors
///
/// Returns [`VaultError::Git`] when the repository cannot be opened.
pub fn open(git_dir: &Path, worktree: &Path) -> Result<GitStore, VaultError> {
    dispatch_in_worktree(git_dir, worktree, open_existing_store)
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

    fn sorted(mut changes: Vec<FileChange>) -> Vec<FileChange> {
        changes.sort_by(|a, b| a.rel.as_str().cmp(b.rel.as_str()));
        changes
    }

    #[test]
    fn walk_history_reproduces_commit_changes_oldest_first() {
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
        std::fs::write(store.worktree().join("b.md"), b"new").expect("write");
        let changes2 = vec![
            FileChange {
                rel: RelPath::parse("a.md"),
                kind: FileEventKind::Modify,
            },
            FileChange {
                rel: RelPath::parse("b.md"),
                kind: FileEventKind::Create,
            },
        ];
        let sha2 = store
            .commit_tree(&changes2, "commit 2")
            .expect("commit")
            .expect("sha");

        std::fs::remove_file(store.worktree().join("a.md")).expect("remove");
        let changes3 = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Delete,
        }];
        let sha3 = store
            .commit_tree(&changes3, "commit 3")
            .expect("commit")
            .expect("sha");

        let history = store.walk_history().expect("history");
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].sha.as_str(), sha1.as_str());
        assert_eq!(sorted(history[0].changes.clone()), sorted(changes1));
        assert_eq!(history[1].sha.as_str(), sha2.as_str());
        assert_eq!(sorted(history[1].changes.clone()), sorted(changes2));
        assert_eq!(history[2].sha.as_str(), sha3.as_str());
        assert_eq!(sorted(history[2].changes.clone()), sorted(changes3));
    }

    #[test]
    fn walk_history_on_empty_repo_is_empty() {
        let dir = TempDir::new().expect("tempdir");
        let store = init_store(&dir);
        assert_eq!(store.walk_history().expect("history"), vec![]);
    }

    #[test]
    fn walk_history_rejects_merge_commits() {
        let dir = TempDir::new().expect("tempdir");
        let store = init_store(&dir);

        std::fs::write(store.worktree().join("a.md"), b"1").expect("write");
        let sha1 = store
            .commit_tree(
                &[FileChange {
                    rel: RelPath::parse("a.md"),
                    kind: FileEventKind::Create,
                }],
                "c1",
            )
            .expect("commit")
            .expect("sha");

        std::fs::write(store.worktree().join("b.md"), b"2").expect("write");
        let sha2 = store
            .commit_tree(
                &[FileChange {
                    rel: RelPath::parse("b.md"),
                    kind: FileEventKind::Create,
                }],
                "c2",
            )
            .expect("commit")
            .expect("sha");

        let parent1 = gix::ObjectId::from_hex(sha1.as_bytes()).expect("parse sha1");
        let parent2 = gix::ObjectId::from_hex(sha2.as_bytes()).expect("parse sha2");
        let tree_id = store
            .repo
            .head_commit()
            .expect("head commit")
            .tree_id()
            .expect("tree id")
            .detach();
        let mut time_buf = gix::date::parse::TimeBuf::default();
        let signature = gix::actor::Signature {
            name: "vault".into(),
            email: "vault@localhost".into(),
            time: gix::date::Time::now_utc(),
        };
        let sig = signature.to_ref(&mut time_buf);
        // `commit_as(..., "HEAD", ...)` verifies the ref's current value against the *first*
        // parent before updating it, so the actual current HEAD (`sha2`) must come first.
        store
            .repo
            .commit_as(sig, sig, "HEAD", "merge", tree_id, [parent2, parent1])
            .expect("merge commit");

        let err = store
            .walk_history()
            .expect_err("should reject merge commit");
        assert!(matches!(err, VaultError::NonLinearHistory { .. }));
    }
}
