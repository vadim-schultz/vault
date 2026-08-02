//! Snapshot use-case.

use crate::config::VaultConfig;
use crate::domain::VaultLayout;
use crate::error::VaultError;
use crate::ports::{Clock, MetaIndex, ObjectStore};
use crate::walk::collect_baseline_changes;

/// Take a baseline snapshot during init.
///
/// # Errors
///
/// Returns [`VaultError`] when walking, committing, or indexing fails.
pub fn baseline(
    layout: &VaultLayout,
    config: &VaultConfig,
    clock: &dyn Clock,
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
) -> Result<(), VaultError> {
    let changes = collect_baseline_changes(layout, config)?;
    if changes.is_empty() {
        return Ok(());
    }
    let _ = commit(layout, &changes, clock, object_store, meta_index)?;
    Ok(())
}

/// Commit `changes` to git and sqlite.
///
/// # Errors
///
/// Returns [`VaultError`] when git or sqlite operations fail.
pub fn commit(
    _layout: &VaultLayout,
    changes: &[crate::domain::FileChange],
    clock: &dyn Clock,
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
) -> Result<Option<crate::domain::CommitSha>, VaultError> {
    if changes.is_empty() {
        return Ok(None);
    }
    let created_at = clock.now().to_rfc3339();
    let message = snapshot_message(changes, &created_at);
    let Some(commit_sha) = object_store.commit(changes, &message)? else {
        return Ok(None);
    };
    meta_index.record_snapshot(&crate::domain::SnapshotRecord {
        commit_sha: commit_sha.clone(),
        created_at,
        changes: changes.to_vec(),
    })?;
    Ok(Some(commit_sha))
}

fn snapshot_message(changes: &[crate::domain::FileChange], created_at: &str) -> String {
    match changes {
        [only] => single_change_message(only, created_at),
        _ => format!("vault: update {} files @ {created_at}", changes.len()),
    }
}

fn single_change_message(change: &crate::domain::FileChange, created_at: &str) -> String {
    format!(
        "vault: {} {} @ {created_at}",
        verb_for(change.kind),
        change.rel.as_str()
    )
}

const fn verb_for(kind: crate::domain::FileEventKind) -> &'static str {
    match kind {
        crate::domain::FileEventKind::Create | crate::domain::FileEventKind::Modify => "update",
        crate::domain::FileEventKind::Delete => "delete",
        crate::domain::FileEventKind::Restore => "restore",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fakes::{FixedClock, InMemoryMetaIndex, InMemoryObjectStore};
    use crate::domain::{FileChange, FileEventKind, RelPath};
    use chrono::TimeZone;

    #[test]
    fn timestamp_comes_from_injected_clock() {
        let clock = FixedClock::at(chrono::Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap());
        let object_store = InMemoryObjectStore::default();
        let meta_index = InMemoryMetaIndex::default();
        let layout = VaultLayout::from_worktree(std::env::temp_dir());
        let changes = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        commit(&layout, &changes, &clock, &object_store, &meta_index).expect("commit");
        let last = meta_index.last_snapshot_time().expect("time");
        assert_eq!(last, Some("2026-06-01T12:00:00+00:00".to_string()));
    }

    #[test]
    fn verb_for_all_kinds() {
        assert_eq!(verb_for(FileEventKind::Create), "update");
        assert_eq!(verb_for(FileEventKind::Modify), "update");
        assert_eq!(verb_for(FileEventKind::Delete), "delete");
        assert_eq!(verb_for(FileEventKind::Restore), "restore");
    }
}
