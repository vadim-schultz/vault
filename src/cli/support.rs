//! Shared, adapter-agnostic CLI marshalling helpers.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::{RelPath, VaultLayout};
use crate::error::VaultError;

/// Global flags shared by every subcommand.
#[derive(Debug, Clone)]
pub struct Global {
    /// Path to the `.vault/` directory (default: `./.vault` under the current directory).
    pub vault_path: Option<PathBuf>,
    /// Enable verbose output.
    pub verbose: bool,
}

/// Convert a raw CLI path argument into a validated [`RelPath`].
///
/// # Errors
///
/// Returns [`VaultError`] when an absolute path falls outside the vault's worktree.
pub fn rel_path_from_cli(layout: &VaultLayout, path: &Path) -> Result<RelPath, VaultError> {
    if path.is_absolute() {
        RelPath::from_worktree(&layout.worktree, path)
    } else {
        RelPath::from_rel(path)
    }
}

/// Run a blocking use-case function on the blocking thread pool.
///
/// # Errors
///
/// Returns an error when the blocking task panics or the use-case itself fails.
pub async fn run_blocking<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T, VaultError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await?
        .map_err(|err| anyhow::anyhow!(err))
}
