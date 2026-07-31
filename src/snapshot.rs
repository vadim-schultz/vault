//! Snapshot commits via gix and sqlite metadata inserts.

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::config::VaultConfig;
use crate::error::VaultError;
use crate::ignore::{exceeds_max_bytes, IgnoreMatcher};
use crate::paths::VaultPaths;
use crate::storage::{git, sqlite};
use crate::walk::collect_baseline_changes;

/// Kind of file change recorded in `file_events`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventKind {
    /// File was created.
    Create,
    /// File was modified.
    Modify,
    /// File was deleted.
    Delete,
}

impl FileEventKind {
    /// Return the sqlite `event_type` string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
        }
    }
}

/// One file change to include in a snapshot commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path relative to the vault worktree.
    pub rel: PathBuf,
    /// Change kind.
    pub kind: FileEventKind,
}

/// Result of a successful snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotResult {
    /// Commit object id (hex).
    pub commit_sha: String,
    /// ISO-8601 UTC timestamp.
    pub created_at: String,
}

/// Take a baseline snapshot during `vault init`.
///
/// # Errors
///
/// Returns [`VaultError`] when walking, committing, or indexing fails.
pub fn baseline_snapshot(
    paths: &VaultPaths,
    config: &VaultConfig,
) -> Result<Option<SnapshotResult>, VaultError> {
    let changes = collect_baseline_changes(paths, config)?;
    if changes.is_empty() {
        return Ok(None);
    }
    commit_changes(paths, &changes)
}

/// Commit `changes` into the vault git store and sqlite index.
///
/// # Errors
///
/// Returns [`VaultError`] when git or sqlite operations fail.
pub fn commit_changes(
    paths: &VaultPaths,
    changes: &[FileChange],
) -> Result<Option<SnapshotResult>, VaultError> {
    if changes.is_empty() {
        return Ok(None);
    }

    let store = git::open(&paths.git_dir_path(), &paths.worktree)?;
    let created_at = Utc::now().to_rfc3339();
    let message = snapshot_message(changes, &created_at);
    let Some(commit_sha) = store.commit_tree(changes, &message)? else {
        return Ok(None);
    };
    sqlite::insert_snapshot(&paths.meta_db_path(), &commit_sha, &created_at, changes)?;
    Ok(Some(SnapshotResult {
        commit_sha,
        created_at,
    }))
}

/// Filter debounced file paths into snapshot changes for one vault.
///
/// # Errors
///
/// Returns [`VaultError`] when ignore matching fails.
pub fn changes_from_paths(
    worktree: &Path,
    rel_paths: &[PathBuf],
    config: &VaultConfig,
) -> Result<Vec<FileChange>, VaultError> {
    let matcher = IgnoreMatcher::from_config(config)?;
    let ctx = NotifyChangeContext {
        worktree,
        matcher: &matcher,
        max_file_bytes: config.watcher.max_file_bytes,
    };
    let mut changes = Vec::new();
    for rel in rel_paths {
        if let Some(change) = change_from_notified_path(rel, &ctx)? {
            changes.push(change);
        }
    }
    Ok(changes)
}

/// Inputs shared by notify-time change handlers.
struct NotifyChangeContext<'a> {
    worktree: &'a Path,
    matcher: &'a IgnoreMatcher,
    max_file_bytes: u64,
}

/// Whether a notified path currently exists as a regular file.
#[derive(Copy, Clone)]
enum PathPresence {
    /// Path is a file on disk.
    File,
    /// Path is absent (or not a file).
    Missing,
}

impl PathPresence {
    fn at(abs: &Path) -> Self {
        if abs.is_file() {
            Self::File
        } else {
            Self::Missing
        }
    }
}

fn change_from_notified_path(
    rel: &Path,
    ctx: &NotifyChangeContext<'_>,
) -> Result<Option<FileChange>, VaultError> {
    if ctx.matcher.is_ignored(rel) {
        return Ok(None);
    }
    let abs = ctx.worktree.join(rel);
    match PathPresence::at(&abs) {
        PathPresence::File => build_modify_change(rel, &abs, ctx),
        PathPresence::Missing => Ok(Some(FileChange {
            rel: rel.to_path_buf(),
            kind: FileEventKind::Delete,
        })),
    }
}

fn build_modify_change(
    rel: &Path,
    abs: &Path,
    ctx: &NotifyChangeContext<'_>,
) -> Result<Option<FileChange>, VaultError> {
    if exceeds_max_bytes(abs, ctx.max_file_bytes)? {
        return Ok(None);
    }
    Ok(Some(FileChange {
        rel: rel.to_path_buf(),
        kind: FileEventKind::Modify,
    }))
}

fn snapshot_message(changes: &[FileChange], created_at: &str) -> String {
    if changes.len() == 1 {
        let path = changes[0].rel.display();
        return format!("vault: update {path} @ {created_at}");
    }
    format!("vault: update {} files @ {created_at}", changes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use crate::paths::InitPaths;
    use crate::storage;

    fn init_vault(dir: &TempDir) -> VaultPaths {
        let paths = InitPaths {
            worktree: dir.path().to_path_buf(),
            vault_dir: dir.path().join(crate::paths::VAULT_DIR),
        };
        storage::git::init(&paths.git_dir_path(), &paths.worktree).expect("git");
        storage::sqlite::init_meta_db(&paths.meta_db_path()).expect("sqlite");
        paths.into()
    }

    #[test]
    fn first_commit_on_unborn_head() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("notes.md"), b"v1").expect("write");
        let paths = init_vault(&dir);
        let changes = vec![FileChange {
            rel: PathBuf::from("notes.md"),
            kind: FileEventKind::Create,
        }];
        let result = commit_changes(&paths, &changes)
            .expect("commit")
            .expect("some");
        assert!(!result.commit_sha.is_empty());
        assert_eq!(
            sqlite::snapshot_count(&paths.meta_db_path()).expect("count"),
            1
        );
    }

    #[test]
    fn modify_after_baseline_advances_head() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("a.md"), b"a").expect("write");
        let paths = init_vault(&dir);
        let baseline = vec![FileChange {
            rel: PathBuf::from("a.md"),
            kind: FileEventKind::Create,
        }];
        commit_changes(&paths, &baseline)
            .expect("baseline")
            .expect("some");
        fs::write(dir.path().join("a.md"), b"a2").expect("write");
        let modify = vec![FileChange {
            rel: PathBuf::from("a.md"),
            kind: FileEventKind::Modify,
        }];
        let result = commit_changes(&paths, &modify)
            .expect("modify")
            .expect("some");
        assert!(!result.commit_sha.is_empty());
        assert_eq!(
            sqlite::snapshot_count(&paths.meta_db_path()).expect("count"),
            2
        );
    }

    #[test]
    fn changes_from_paths_classifies_modify_and_delete() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("keep.md");
        fs::write(&file, b"x").expect("write");
        let config = VaultConfig::defaults();

        let changes =
            changes_from_paths(dir.path(), &[PathBuf::from("keep.md")], &config).expect("modify");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, FileEventKind::Modify);

        fs::remove_file(file).expect("remove");
        let changes =
            changes_from_paths(dir.path(), &[PathBuf::from("keep.md")], &config).expect("delete");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, FileEventKind::Delete);
    }

    #[test]
    fn changes_from_paths_skips_ignored_and_oversized() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("notes.md.swp"), b"x").expect("swp");
        fs::write(dir.path().join("big.bin"), vec![0_u8; 11]).expect("big");
        let mut config = VaultConfig::defaults();
        config.watcher.max_file_bytes = 10;

        let changes = changes_from_paths(
            dir.path(),
            &[PathBuf::from("notes.md.swp"), PathBuf::from("big.bin")],
            &config,
        )
        .expect("changes");
        assert!(changes.is_empty());
    }
}
