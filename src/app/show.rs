//! `vault show` use-case.

use crate::app::file_diff;
use crate::domain::{snapshot_message, CommitReport, CommitSha, RelPath};
use crate::error::VaultError;
use crate::ports::{MetaIndex, ObjectStore};

/// What `vault show` prints: a single file's raw bytes, or a `git show`-shaped report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowOutput {
    /// Raw content of one file at the resolved commit (today's unchanged behavior).
    Content(Vec<u8>),
    /// Header + full diff per file, for the whole vault or a directory subtree.
    Report(CommitReport),
}

enum Scope {
    File(RelPath),
    Directory(RelPath),
    WholeVault,
}

/// Resolve `path`/`at` into a [`ShowOutput`].
///
/// `PATH` omitted resolves to a whole-vault report; an exact (ever-)tracked path resolves to
/// today's raw content dump; a strict prefix of tracked paths resolves to a directory-scoped
/// report; anything else fails with [`VaultError::PathNotTrackedAt`].
///
/// # Errors
///
/// Returns [`VaultError::NoSnapshotAt`] when no snapshot exists at or before `at`, or
/// [`VaultError::PathNotTrackedAt`] when `path` matches neither a tracked file nor a directory
/// prefix of one.
pub fn run(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: Option<&RelPath>,
    at: &str,
) -> Result<ShowOutput, VaultError> {
    match resolve_scope(meta_index, path, at)? {
        Scope::File(path) => {
            read_file_at(object_store, meta_index, &path, at).map(ShowOutput::Content)
        }
        Scope::Directory(prefix) => {
            build_report(object_store, meta_index, at, Some(&prefix)).map(ShowOutput::Report)
        }
        Scope::WholeVault => {
            build_report(object_store, meta_index, at, None).map(ShowOutput::Report)
        }
    }
}

/// Return one file's raw content at or before `at`, unconditionally (no scope disambiguation).
///
/// Used directly by `vault restore`, which always names an exact path.
///
/// # Errors
///
/// Returns [`VaultError::NoSnapshotAt`] when no snapshot exists at or before `at`, or
/// [`VaultError::PathNotTrackedAt`] when the path did not exist in that snapshot's tree.
pub fn read_file_at(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
    at: &str,
) -> Result<Vec<u8>, VaultError> {
    let commit = resolve_commit(meta_index, at)?;
    read_tracked_blob(object_store, &commit, path, at)
}

fn resolve_scope(
    meta_index: &dyn MetaIndex,
    path: Option<&RelPath>,
    at: &str,
) -> Result<Scope, VaultError> {
    let Some(path) = path else {
        return Ok(Scope::WholeVault);
    };
    let all_paths = meta_index.all_paths()?;
    if all_paths.contains(path) {
        return Ok(Scope::File(path.clone()));
    }
    if is_directory_prefix(&all_paths, path) {
        return Ok(Scope::Directory(path.clone()));
    }
    Err(VaultError::PathNotTrackedAt {
        path: path.as_str().to_string(),
        at: at.to_string(),
    })
}

fn is_directory_prefix(all_paths: &[RelPath], path: &RelPath) -> bool {
    let prefix = format!("{}/", path.as_str());
    all_paths.iter().any(|p| p.as_str().starts_with(&prefix))
}

fn build_report(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    at: &str,
    prefix: Option<&RelPath>,
) -> Result<CommitReport, VaultError> {
    let commit = resolve_commit(meta_index, at)?;
    let created_at = meta_index
        .created_at_for(&commit)?
        .unwrap_or_else(|| at.to_string());
    let changeset = scope_to_prefix(meta_index.changeset(&commit)?, prefix);
    let message = snapshot_message(&changeset, &created_at);
    let files = file_diff::resolve_files(object_store, meta_index, &commit, &changeset)?;
    Ok(CommitReport { message, files })
}

fn scope_to_prefix(
    changeset: Vec<crate::domain::FileChange>,
    prefix: Option<&RelPath>,
) -> Vec<crate::domain::FileChange> {
    match prefix {
        Some(prefix) => {
            let prefix = format!("{}/", prefix.as_str());
            changeset
                .into_iter()
                .filter(|c| c.rel.as_str().starts_with(&prefix))
                .collect()
        }
        None => changeset,
    }
}

fn resolve_commit(meta_index: &dyn MetaIndex, at: &str) -> Result<CommitSha, VaultError> {
    meta_index
        .resolve_at(at)?
        .ok_or_else(|| VaultError::NoSnapshotAt { at: at.to_string() })
}

fn read_tracked_blob(
    object_store: &dyn ObjectStore,
    commit: &CommitSha,
    path: &RelPath,
    at: &str,
) -> Result<Vec<u8>, VaultError> {
    object_store
        .read_blob(commit, path)?
        .ok_or_else(|| VaultError::PathNotTrackedAt {
            path: path.as_str().to_string(),
            at: at.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fakes::FixedClock;
    use crate::adapters::{GixObjectStore, SqliteMetaIndex};
    use crate::app::snapshot;
    use crate::domain::VaultLayout;
    use chrono::{TimeZone, Utc};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn show_returns_content_at_commit() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = crate::config::VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());

        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit 1");

        fs::write(layout.worktree.join("a.md"), b"v2").expect("update");
        let changes2 = vec![crate::domain::FileChange {
            rel: RelPath::parse("a.md"),
            kind: crate::domain::FileEventKind::Modify,
        }];
        let clock2 = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 2, 9, 0, 0).unwrap());
        snapshot::commit(&layout, &changes2, &clock2, &obj_store, &meta_idx).expect("commit 2");

        assert_eq!(
            run(
                &obj_store,
                &meta_idx,
                Some(&RelPath::parse("a.md")),
                "2026-06-01T12:00:00+00:00"
            )
            .expect("show"),
            ShowOutput::Content(b"v1".to_vec())
        );
        assert_eq!(
            run(
                &obj_store,
                &meta_idx,
                Some(&RelPath::parse("a.md")),
                "2026-06-02T12:00:00+00:00"
            )
            .expect("show"),
            ShowOutput::Content(b"v2".to_vec())
        );
    }

    #[test]
    fn show_before_any_snapshot_fails() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = crate::config::VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());

        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        match run(
            &obj_store,
            &meta_idx,
            Some(&RelPath::parse("a.md")),
            "2020-01-01T00:00:00+00:00",
        ) {
            Err(VaultError::NoSnapshotAt { .. }) => {}
            other => panic!("expected NoSnapshotAt, got {other:?}"),
        }
    }

    #[test]
    fn show_untracked_path_fails() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = crate::config::VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());

        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        match run(
            &obj_store,
            &meta_idx,
            Some(&RelPath::parse("missing.md")),
            "2026-06-01T12:00:00+00:00",
        ) {
            Err(VaultError::PathNotTrackedAt { .. }) => {}
            other => panic!("expected PathNotTrackedAt, got {other:?}"),
        }
    }

    #[test]
    fn show_with_no_path_returns_whole_vault_report() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");
        fs::write(layout.worktree.join("b.md"), b"b1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = crate::config::VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());
        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        let output = run(&obj_store, &meta_idx, None, "2026-06-01T12:00:00+00:00").expect("show");
        let ShowOutput::Report(report) = output else {
            panic!("expected a report")
        };
        assert!(report.message.starts_with("update 2 files @"));
        assert_eq!(report.files.len(), 2);
    }

    #[test]
    fn show_with_directory_path_scopes_report_to_that_subtree() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(layout.worktree.join("sub")).expect("mkdir sub");
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");
        fs::write(layout.worktree.join("sub/child.md"), b"c1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = crate::config::VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());
        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        let output = run(
            &obj_store,
            &meta_idx,
            Some(&RelPath::parse("sub")),
            "2026-06-01T12:00:00+00:00",
        )
        .expect("show");
        let ShowOutput::Report(report) = output else {
            panic!("expected a report")
        };
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path.as_str(), "sub/child.md");
    }
}
