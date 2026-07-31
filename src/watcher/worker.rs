//! Per-vault snapshot workers.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::VaultError;
use crate::paths::VaultPaths;
use crate::snapshot::{changes_from_paths, commit_changes};
use crate::watcher::router::WatchedVault;

static COMMIT_LOCK: Mutex<()> = Mutex::new(());

/// Commit a debounced batch of relative paths for one vault.
///
/// # Errors
///
/// Returns [`VaultError`] when change detection or snapshot fails.
pub fn commit_batch(vault: &WatchedVault, rel_paths: &[PathBuf]) -> Result<(), VaultError> {
    let _guard = COMMIT_LOCK
        .lock()
        .map_err(|_| VaultError::Notify("commit lock poisoned".to_string()))?;
    let changes = changes_from_paths(&vault.root, rel_paths, &vault.config)?;
    if changes.is_empty() {
        return Ok(());
    }
    let paths = VaultPaths {
        worktree: vault.root.clone(),
        vault_dir: vault.paths.vault_dir.clone(),
    };
    let _ = commit_changes(&paths, &changes)?;
    Ok(())
}
