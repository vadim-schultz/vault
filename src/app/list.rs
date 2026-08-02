//! `vault list` use-case.

use crate::domain::TrackedFile;
use crate::error::VaultError;
use crate::ports::MetaIndex;

/// List tracked files and their latest snapshot timestamp.
///
/// # Errors
///
/// Returns [`VaultError`] when the metadata index cannot be read.
pub fn run(meta_index: &dyn MetaIndex) -> Result<Vec<TrackedFile>, VaultError> {
    meta_index.list_tracked_files()
}
