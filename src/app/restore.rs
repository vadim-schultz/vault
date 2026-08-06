//! `vault restore` use-case.

use crate::app::snapshot;
use crate::domain::{CommitSha, FileChange, FileEventKind, RelPath, VaultLayout};
use crate::error::VaultError;
use crate::ports::{Clock, MetaIndex, ObjectStore};

/// Outcome of a restore, for CLI messaging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    /// Bytes written to the working file (0 on a dry run).
    pub bytes_written: usize,
    /// The new commit created for the restore, `None` when content was already current or this was a dry run.
    pub commit_sha: Option<CommitSha>,
}

/// Restore `path` to its content at or before `at`, then record the restore as its own snapshot.
///
/// When `dry_run` is `true`, resolves and validates but writes and commits nothing.
///
/// # Errors
///
/// Returns [`VaultError::NoSnapshotAt`] / [`VaultError::PathNotTrackedAt`] on resolution
/// failure, or [`VaultError::Io`] when the file cannot be written.
pub fn run(
    layout: &VaultLayout,
    clock: &dyn Clock,
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
    at: &str,
    dry_run: bool,
) -> Result<RestoreOutcome, VaultError> {
    let content = crate::app::show::read_file_at(object_store, meta_index, path, at)?;
    if dry_run {
        return Ok(RestoreOutcome {
            bytes_written: 0,
            commit_sha: None,
        });
    }
    write_working_file(layout, path, &content)?;
    let commit_sha = commit_restore(layout, clock, object_store, meta_index, path)?;
    Ok(RestoreOutcome {
        bytes_written: content.len(),
        commit_sha,
    })
}

fn write_working_file(
    layout: &VaultLayout,
    path: &RelPath,
    content: &[u8],
) -> Result<(), VaultError> {
    let abs = layout.worktree.join(path.to_path());
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(abs, content).map_err(VaultError::Io)
}

fn commit_restore(
    layout: &VaultLayout,
    clock: &dyn Clock,
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
) -> Result<Option<CommitSha>, VaultError> {
    let changes = [FileChange {
        rel: path.clone(),
        kind: FileEventKind::Restore,
    }];
    snapshot::commit(layout, &changes, clock, object_store, meta_index)
}
