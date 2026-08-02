//! `vault show` use-case.

use crate::domain::RelPath;
use crate::error::VaultError;
use crate::ports::{MetaIndex, ObjectStore};

/// Return file content as it existed at or before `at`.
///
/// # Errors
///
/// Returns [`VaultError::NoSnapshotAt`] when no snapshot exists at or before `at`, or
/// [`VaultError::PathNotTrackedAt`] when the path did not exist in that snapshot's tree.
pub fn run(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
    at: &str,
) -> Result<Vec<u8>, VaultError> {
    let commit = resolve_commit(meta_index, at)?;
    read_tracked_blob(object_store, &commit, path, at)
}

fn resolve_commit(
    meta_index: &dyn MetaIndex,
    at: &str,
) -> Result<crate::domain::CommitSha, VaultError> {
    meta_index
        .resolve_at(at)?
        .ok_or_else(|| VaultError::NoSnapshotAt { at: at.to_string() })
}

fn read_tracked_blob(
    object_store: &dyn ObjectStore,
    commit: &crate::domain::CommitSha,
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
    use crate::config::VaultConfig;
    use crate::domain::VaultLayout;
    use chrono::{TimeZone, Utc};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn show_returns_content_at_commit() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(&layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());

        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit 1");

        fs::write(&layout.worktree.join("a.md"), b"v2").expect("update");
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
                &RelPath::parse("a.md"),
                "2026-06-01T12:00:00+00:00"
            )
            .expect("show"),
            b"v1"
        );
        assert_eq!(
            run(
                &obj_store,
                &meta_idx,
                &RelPath::parse("a.md"),
                "2026-06-02T12:00:00+00:00"
            )
            .expect("show"),
            b"v2"
        );
    }

    #[test]
    fn show_before_any_snapshot_fails() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(&layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());

        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        match run(
            &obj_store,
            &meta_idx,
            &RelPath::parse("a.md"),
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
        fs::write(&layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let config = VaultConfig::defaults();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());

        let changes = crate::walk::collect_baseline_changes(&layout, &config).expect("walk");
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        match run(
            &obj_store,
            &meta_idx,
            &RelPath::parse("missing.md"),
            "2026-06-01T12:00:00+00:00",
        ) {
            Err(VaultError::PathNotTrackedAt { .. }) => {}
            other => panic!("expected PathNotTrackedAt, got {other:?}"),
        }
    }
}
