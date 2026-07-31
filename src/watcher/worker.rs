//! Per-vault snapshot workers.

use crate::adapters::{GixObjectStore, SqliteMetaIndex, SystemClock};
use crate::app::snapshot;
use crate::domain::RelPath;
use crate::error::VaultError;
use crate::snapshot::changes_from_rel_paths;
use crate::watcher::router::WatchedVault;

/// Commit a debounced batch of relative paths for one vault.
///
/// # Errors
///
/// Returns [`VaultError`] when change detection or snapshot fails.
pub fn commit_batch(vault: &WatchedVault, rel_paths: &[RelPath]) -> Result<(), VaultError> {
    let changes = changes_from_rel_paths(
        &vault.root,
        rel_paths,
        &vault.ignore,
        vault.config.watcher.max_file_bytes,
    )?;
    if changes.is_empty() {
        return Ok(());
    }
    let object_store = GixObjectStore::open(&vault.layout)?;
    let meta_index = SqliteMetaIndex::open(vault.layout.meta_db_path())?;
    snapshot::commit(
        &vault.layout,
        &changes,
        &SystemClock,
        &object_store,
        &meta_index,
    )
}
