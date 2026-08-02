//! `vault diff` use-case.

use crate::domain::{RelPath, VaultLayout};
use crate::error::VaultError;
use crate::ports::{MetaIndex, ObjectStore};

/// Resolved diff inputs, ready for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOutcome {
    /// Label for left side.
    pub left_label: String,
    /// Label for right side.
    pub right_label: String,
    /// Content on left (None if missing).
    pub left: Option<Vec<u8>>,
    /// Content on right (None if missing).
    pub right: Option<Vec<u8>>,
}

/// Resolve both sides of a diff for `path`.
///
/// `at`/`to` are already-resolved UTC RFC3339 strings. When both are None, compares the latest
/// snapshot against the working tree. When only `at` is set, compares that snapshot against the
/// working tree. The CLI layer rejects `to` without `at`.
///
/// # Errors
///
/// Returns [`VaultError::NoSnapshotAt`] when an explicit `at`/`to` resolves to no snapshot.
pub fn run(
    layout: &VaultLayout,
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
    at: Option<&str>,
    to: Option<&str>,
) -> Result<DiffOutcome, VaultError> {
    let (left_label, left) = resolve_side(object_store, meta_index, path, at, at.is_some())?;
    let (right_label, right) = resolve_right(layout, object_store, meta_index, path, to)?;
    Ok(DiffOutcome {
        left_label,
        right_label,
        left,
        right,
    })
}

fn resolve_right(
    layout: &VaultLayout,
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
    to: Option<&str>,
) -> Result<(String, Option<Vec<u8>>), VaultError> {
    match to {
        Some(to) => resolve_side(object_store, meta_index, path, Some(to), true),
        None => Ok(("working tree".to_string(), read_working_file(layout, path)?)),
    }
}

fn resolve_side(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
    at: Option<&str>,
    explicit: bool,
) -> Result<(String, Option<Vec<u8>>), VaultError> {
    let Some(at) = resolve_timestamp(meta_index, at)? else {
        return Ok(no_snapshot_yet());
    };
    resolve_at_timestamp(object_store, meta_index, path, at, explicit)
}

fn resolve_timestamp(
    meta_index: &dyn MetaIndex,
    at: Option<&str>,
) -> Result<Option<String>, VaultError> {
    match at {
        Some(at) => Ok(Some(at.to_string())),
        None => meta_index.last_snapshot_time(),
    }
}

fn resolve_at_timestamp(
    object_store: &dyn ObjectStore,
    meta_index: &dyn MetaIndex,
    path: &RelPath,
    at: String,
    explicit: bool,
) -> Result<(String, Option<Vec<u8>>), VaultError> {
    match meta_index.resolve_at(&at)? {
        Some(commit) => Ok((at, object_store.read_blob(&commit, path)?)),
        None if explicit => Err(VaultError::NoSnapshotAt { at }),
        None => Ok(no_snapshot_yet()),
    }
}

fn no_snapshot_yet() -> (String, Option<Vec<u8>>) {
    ("no snapshot yet".to_string(), None)
}

fn read_working_file(layout: &VaultLayout, path: &RelPath) -> Result<Option<Vec<u8>>, VaultError> {
    let abs = layout.worktree.join(path.to_path());
    match std::fs::read(abs) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(VaultError::Io(e)),
    }
}
