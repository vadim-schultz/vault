//! `vault log` use-case.

use crate::domain::{RelPath, SnapshotEntry};
use crate::error::VaultError;
use crate::ports::MetaIndex;

/// List snapshot history, optionally scoped to `path`, newest first.
///
/// # Errors
///
/// Returns [`VaultError`] when the metadata index cannot be read.
pub fn run(
    meta_index: &dyn MetaIndex,
    path: Option<&RelPath>,
) -> Result<Vec<SnapshotEntry>, VaultError> {
    meta_index.list_snapshots(path)
}
