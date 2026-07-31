//! Snapshot commits via gix and sqlite metadata inserts.

use std::path::Path;

use crate::adapters::probe_path;
use crate::config::VaultConfig;
use crate::domain::{CommitSha, FileChange, RelPath, SnapshotRecord, SnapshotResult, VaultLayout};
use crate::error::VaultError;
use crate::ignore::IgnoreMatcher;
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
/// `ignore` is the matcher already compiled for the vault, so a debounce batch does
/// not rebuild the glob set.
///
/// # Errors
///
/// Returns [`VaultError`] when a path exists but cannot be inspected.
pub fn changes_from_rel_paths(
    worktree: &Path,
    rel_paths: &[RelPath],
    ignore: &IgnoreMatcher,
    max_file_bytes: u64,
) -> Result<Vec<FileChange>, VaultError> {
    let mut changes = Vec::new();
    for rel in rel_paths {
        if ignore.is_ignored(rel) {
            continue;
        }
        let found = probe_path(&worktree.join(rel.to_path()))?;
        let Some(kind) = found.classify(max_file_bytes) else {
            continue;
        };
        changes.push(FileChange {
            rel: rel.clone(),
            kind,
        });
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
    use crate::domain::FileEventKind;
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

    fn matcher(config: &VaultConfig) -> IgnoreMatcher {
        IgnoreMatcher::from_config(config).expect("matcher")
    }

    #[test]
    fn changes_from_rel_paths_classifies_modify_and_delete() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("keep.md");
        fs::write(&file, b"x").expect("write");
        let config = VaultConfig::defaults();
        let ignore = matcher(&config);
        let max = config.watcher.max_file_bytes;

        let changes =
            changes_from_rel_paths(dir.path(), &[RelPath::parse("keep.md")], &ignore, max)
                .expect("modify");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, FileEventKind::Modify);

        fs::remove_file(file).expect("remove");
        let changes =
            changes_from_rel_paths(dir.path(), &[RelPath::parse("keep.md")], &ignore, max)
                .expect("delete");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, FileEventKind::Delete);
    }

    #[test]
    fn directory_event_does_not_become_a_delete() {
        let dir = TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join("research")).expect("mkdir");
        fs::write(dir.path().join("research").join("sources.md"), b"x").expect("write");
        let config = VaultConfig::defaults();

        let changes = changes_from_rel_paths(
            dir.path(),
            &[RelPath::parse("research")],
            &matcher(&config),
            config.watcher.max_file_bytes,
        )
        .expect("changes");

        assert!(
            changes.is_empty(),
            "a directory event must not produce a change, got {changes:?}"
        );
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
            &matcher(&config),
            config.watcher.max_file_bytes,
        )
        .expect("changes");
        assert!(changes.is_empty());
    }
}
