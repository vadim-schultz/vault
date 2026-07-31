//! `vault init` orchestration.

use std::path::{Path, PathBuf};

use crate::config::VaultConfig;
use crate::daemon;
use crate::error::VaultError;
use crate::paths::{is_initialized, InitPaths};
use crate::registry;
use crate::snapshot;
use crate::storage;

const README: &str = "\
Vault storage (recovery guide)
=============================

This directory is managed by the vault CLI. You normally do not edit it.

Layout
------

  config.toml   Watch roots and ignore patterns
  .git/         Git object store (file content history)
  meta.db       SQLite index (paths, timestamps, commit SHAs)

Global registry
---------------

  vault init registers this directory in the user-wide registry.toml
  (see docs). A singleton background daemon watches all registered vaults.

Inspect without vault (optional)
--------------------------------

  git --git-dir=.vault/.git log --oneline
  sqlite3 .vault/meta.db \".schema\"

Vault does not invoke git or sqlite3 internally; the on-disk layout is standard.

Daily use
---------

  vault show PATH --at DATE
  vault restore PATH --at DATE
";

/// Options controlling post-init daemon startup.
#[derive(Debug, Clone, Copy, Default)]
pub struct InitOptions {
    /// Skip service install and daemon start.
    pub no_service: bool,
}

/// Initialize a new vault at `paths`.
///
/// # Errors
///
/// Returns [`VaultError::AlreadyInitialized`] when init markers exist,
/// or other errors when artifact creation fails.
pub fn run(paths: &InitPaths, options: InitOptions) -> Result<(), VaultError> {
    if is_initialized(&paths.vault_dir) {
        return Err(VaultError::AlreadyInitialized {
            path: paths.vault_dir.clone(),
        });
    }

    std::fs::create_dir_all(&paths.vault_dir)?;

    storage::git::init(&paths.git_dir_path(), &paths.worktree)?;
    storage::sqlite::init_meta_db(&paths.meta_db_path())?;
    write_readme(&paths.readme_path())?;
    VaultConfig::defaults().write_to(&paths.config_path())?;

    let config = VaultConfig::load(&paths.config_path())?;
    let vault_paths = paths.clone().into();
    snapshot::baseline_snapshot(&vault_paths, &config)?;

    registry::register(&paths.worktree)?;

    if !options.no_service && !crate::paths::skip_service() {
        daemon::ensure_running()?;
    }

    Ok(())
}

/// Initialize a vault from CLI-style arguments.
///
/// Resolves `vault_path`, applies service skip env overrides, and runs init.
///
/// # Errors
///
/// Returns [`VaultError`] when path resolution or initialization fails.
pub fn initialize(vault_path: Option<PathBuf>, no_service: bool) -> Result<InitPaths, VaultError> {
    let paths = crate::paths::resolve_init(vault_path)?;
    let options = InitOptions {
        no_service: no_service || crate::paths::skip_service(),
    };
    run(&paths, options)?;
    Ok(paths)
}

fn write_readme(path: &Path) -> Result<(), VaultError> {
    std::fs::write(path, README)?;
    Ok(())
}
