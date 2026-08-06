//! Resolve a changed file's previous/current content for a commit.
//!
//! Shared by `log`'s diffstat/`--verbose` rendering and `show`'s report mode — both need the
//! same "what did this path look like just before, and at, this commit" lookup.

use crate::domain::{CommitSha, FileChange, FileVersionDiff};
use crate::error::VaultError;
use crate::ports::{MetaIndex, ObjectStore};

/// Resolve `changes`' before/after content at `commit`.
///
/// # Errors
///
/// Returns [`VaultError`] when the object store or metadata index cannot be read.
pub fn resolve_files(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    commit: &CommitSha,
    changes: &[FileChange],
) -> Result<Vec<FileVersionDiff>, VaultError> {
    changes
        .iter()
        .map(|change| resolve_one(object_store, meta_index, commit, change))
        .collect()
}

fn resolve_one(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    commit: &CommitSha,
    change: &FileChange,
) -> Result<FileVersionDiff, VaultError> {
    let current = object_store.read_blob(commit, &change.rel)?;
    let previous = match meta_index.previous_commit_for(&change.rel, commit)? {
        Some(prev) => object_store.read_blob(&prev, &change.rel)?,
        None => None,
    };
    Ok(FileVersionDiff {
        path: change.rel.clone(),
        previous,
        current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fakes::FixedClock;
    use crate::adapters::{GixObjectStore, SqliteMetaIndex};
    use crate::app::snapshot;
    use crate::domain::{FileEventKind, RelPath, VaultLayout};
    use chrono::{TimeZone, Utc};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn resolves_previous_and_current_content() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let clock1 = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());
        let create = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        snapshot::commit(&layout, &create, &clock1, &obj_store, &meta_idx).expect("commit 1");

        fs::write(layout.worktree.join("a.md"), b"v2").expect("update");
        let modify = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Modify,
        }];
        let clock2 = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 2, 9, 0, 0).unwrap());
        let second =
            snapshot::commit(&layout, &modify, &clock2, &obj_store, &meta_idx).expect("commit 2");
        let commit_sha = second.expect("some commit");

        let resolved = resolve_files(&obj_store, &meta_idx, &commit_sha, &modify).expect("resolve");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].previous, Some(b"v1".to_vec()));
        assert_eq!(resolved[0].current, Some(b"v2".to_vec()));
    }

    #[test]
    fn first_commit_has_no_previous_content() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");

        let obj_store = GixObjectStore::init(&layout).expect("init git");
        let meta_idx = SqliteMetaIndex::open(layout.meta_db_path()).expect("init db");
        let clock = FixedClock::at(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap());
        let create = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        let first =
            snapshot::commit(&layout, &create, &clock, &obj_store, &meta_idx).expect("commit");
        let commit_sha = first.expect("some commit");

        let resolved = resolve_files(&obj_store, &meta_idx, &commit_sha, &create).expect("resolve");
        assert_eq!(resolved[0].previous, None);
        assert_eq!(resolved[0].current, Some(b"v1".to_vec()));
    }
}
