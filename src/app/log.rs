//! `vault log` use-case.

use crate::app::file_diff;
use crate::domain::{snapshot_message, CommitReport, FileChange, RelPath, SnapshotEntry};
use crate::error::VaultError;
use crate::ports::{MetaIndex, ObjectStore};

/// List snapshot history, optionally scoped to `path`, newest first, each resolved to a
/// ready-to-render header and per-file diff content.
///
/// # Errors
///
/// Returns [`VaultError`] when the metadata index or object store cannot be read.
pub fn run(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: Option<&RelPath>,
) -> Result<Vec<CommitReport>, VaultError> {
    meta_index
        .list_snapshots(path)?
        .iter()
        .map(|entry| build_report(object_store, meta_index, path, entry))
        .collect()
}

fn build_report(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: Option<&RelPath>,
    entry: &SnapshotEntry,
) -> Result<CommitReport, VaultError> {
    let changeset = meta_index.changeset(&entry.commit_sha)?;
    let message = snapshot_message(&changeset, &entry.created_at);
    let scoped = scope_to_path(changeset, path);
    let files = file_diff::resolve_files(object_store, meta_index, &entry.commit_sha, &scoped)?;
    Ok(CommitReport { message, files })
}

fn scope_to_path(changeset: Vec<FileChange>, path: Option<&RelPath>) -> Vec<FileChange> {
    match path {
        Some(p) => changeset.into_iter().filter(|c| &c.rel == p).collect(),
        None => changeset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fakes::FixedClock;
    use crate::adapters::{GixObjectStore, SqliteMetaIndex};
    use crate::app::snapshot;
    use crate::domain::{FileEventKind, VaultLayout};
    use chrono::{TimeZone, Utc};
    use std::fs;
    use tempfile::TempDir;

    fn seeded_vault() -> (TempDir, VaultLayout, GixObjectStore, SqliteMetaIndex) {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");
        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        (dir, layout, obj_store, meta_idx)
    }

    #[test]
    fn unscoped_log_has_full_changeset_header_and_files() {
        let (_dir, layout, obj_store, meta_idx) = seeded_vault();
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());
        let changes = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        let reports = run(&obj_store, &meta_idx, None).expect("log");
        assert_eq!(reports.len(), 1);
        assert!(reports[0].message.starts_with("update a.md @"));
        assert_eq!(reports[0].files.len(), 1);
        assert_eq!(reports[0].files[0].path.as_str(), "a.md");
        assert_eq!(reports[0].files[0].current, Some(b"v1".to_vec()));
    }

    #[test]
    fn scoped_log_filters_files_but_keeps_full_commit_header() {
        let (_dir, layout, obj_store, meta_idx) = seeded_vault();
        fs::write(layout.worktree.join("b.md"), b"b1").expect("write b");
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());
        let changes = vec![
            FileChange {
                rel: RelPath::parse("a.md"),
                kind: FileEventKind::Create,
            },
            FileChange {
                rel: RelPath::parse("b.md"),
                kind: FileEventKind::Create,
            },
        ];
        snapshot::commit(&layout, &changes, &clock, &obj_store, &meta_idx).expect("commit");

        let reports = run(&obj_store, &meta_idx, Some(&RelPath::parse("a.md"))).expect("log");
        assert_eq!(reports.len(), 1);
        assert!(reports[0].message.starts_with("update 2 files @"));
        assert_eq!(reports[0].files.len(), 1);
        assert_eq!(reports[0].files[0].path.as_str(), "a.md");
    }
}
