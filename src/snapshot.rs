//! Snapshot commits via gix and sqlite metadata inserts.

use std::path::Path;

use crate::config::VaultConfig;
use crate::domain::{
    CommitSha, FileChange, FileEventKind, RelPath, SnapshotRecord, SnapshotResult, VaultLayout,
};
use crate::error::VaultError;
use crate::ignore::{exceeds_max_bytes, IgnoreMatcher};
use crate::storage::{git, sqlite};
use crate::walk::collect_baseline_changes;

/// Take a baseline snapshot during `vault init`.
///
/// # Errors
///
/// Returns [`VaultError`] when walking, committing, or indexing fails.
pub fn baseline_snapshot(
    layout: &VaultLayout,
    config: &VaultConfig,
) -> Result<Option<SnapshotResult>, VaultError> {
    let changes = collect_baseline_changes(layout, config)?;
    if changes.is_empty() {
        return Ok(None);
    }
    commit_changes(layout, &changes)
}

/// Commit `changes` into the vault git store and sqlite index.
///
/// # Errors
///
/// Returns [`VaultError`] when git or sqlite operations fail.
pub fn commit_changes(
    layout: &VaultLayout,
    changes: &[FileChange],
) -> Result<Option<SnapshotResult>, VaultError> {
    if changes.is_empty() {
        return Ok(None);
    }

    let store = git::open(&layout.git_dir_path(), &layout.worktree)?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let message = snapshot_message(changes, &created_at);
    let Some(commit_sha) = store.commit_tree(changes, &message)? else {
        return Ok(None);
    };
    let record = SnapshotRecord {
        commit_sha: CommitSha(commit_sha.clone()),
        created_at: created_at.clone(),
        changes: changes.to_vec(),
    };
    sqlite::insert_snapshot(&layout.meta_db_path(), &record)?;
    Ok(Some(SnapshotResult {
        commit_sha: CommitSha(commit_sha),
        created_at,
    }))
}

/// Classify notified relative paths into snapshot changes.
///
/// # Errors
///
/// Returns [`VaultError`] when ignore matching fails.
pub fn changes_from_rel_paths(
    worktree: &Path,
    rel_paths: &[RelPath],
    config: &VaultConfig,
) -> Result<Vec<FileChange>, VaultError> {
    let matcher = IgnoreMatcher::from_config(config)?;
    let mut changes = Vec::new();
    for rel in rel_paths {
        if matcher.is_ignored(rel) {
            continue;
        }
        let abs = worktree.join(rel.to_path());
        if abs.is_file() {
            if exceeds_max_bytes(&abs, config.watcher.max_file_bytes)? {
                continue;
            }
            changes.push(FileChange {
                rel: rel.clone(),
                kind: FileEventKind::Modify,
            });
        } else {
            changes.push(FileChange {
                rel: rel.clone(),
                kind: FileEventKind::Delete,
            });
        }
    }
    Ok(changes)
}

fn snapshot_message(changes: &[FileChange], created_at: &str) -> String {
    if changes.len() == 1 {
        let path = changes[0].rel.as_str();
        return format!("vault: update {path} @ {created_at}");
    }
    format!("vault: update {} files @ {created_at}", changes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_vault(dir: &TempDir) -> VaultLayout {
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        crate::storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git");
        crate::storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite");
        layout
    }

    #[test]
    fn first_commit_on_unborn_head() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("notes.md"), b"v1").expect("write");
        let layout = init_vault(&dir);
        let changes = vec![FileChange {
            rel: RelPath::parse("notes.md"),
            kind: FileEventKind::Create,
        }];
        let result = commit_changes(&layout, &changes)
            .expect("commit")
            .expect("some");
        assert!(!result.commit_sha.as_str().is_empty());
        assert_eq!(
            sqlite::snapshot_count(&layout.meta_db_path()).expect("count"),
            1
        );
    }

    #[test]
    fn modify_after_baseline_advances_head() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("a.md"), b"a").expect("write");
        let layout = init_vault(&dir);
        let baseline = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        commit_changes(&layout, &baseline)
            .expect("baseline")
            .expect("some");
        fs::write(dir.path().join("a.md"), b"a2").expect("write");
        let modify = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Modify,
        }];
        let result = commit_changes(&layout, &modify)
            .expect("modify")
            .expect("some");
        assert!(!result.commit_sha.as_str().is_empty());
        assert_eq!(
            sqlite::snapshot_count(&layout.meta_db_path()).expect("count"),
            2
        );
    }

    #[test]
    fn changes_from_rel_paths_classifies_modify_and_delete() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("keep.md");
        fs::write(&file, b"x").expect("write");
        let config = VaultConfig::defaults();

        let changes = changes_from_rel_paths(dir.path(), &[RelPath::parse("keep.md")], &config)
            .expect("modify");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, FileEventKind::Modify);

        fs::remove_file(file).expect("remove");
        let changes = changes_from_rel_paths(dir.path(), &[RelPath::parse("keep.md")], &config)
            .expect("delete");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, FileEventKind::Delete);
    }

    #[test]
    fn changes_from_rel_paths_skips_ignored_and_oversized() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("notes.md.swp"), b"x").expect("swp");
        fs::write(dir.path().join("big.bin"), vec![0_u8; 11]).expect("big");
        let mut config = VaultConfig::defaults();
        config.watcher.max_file_bytes = 10;

        let changes = changes_from_rel_paths(
            dir.path(),
            &[RelPath::parse("notes.md.swp"), RelPath::parse("big.bin")],
            &config,
        )
        .expect("changes");
        assert!(changes.is_empty());
    }
}
