//! `vault init` orchestration.

use std::path::Path;

use crate::config::VaultConfig;
use crate::error::VaultError;
use crate::paths::{is_initialized, InitPaths};
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

/// Initialize a new vault at `paths`.
///
/// # Errors
///
/// Returns [`VaultError::AlreadyInitialized`] when init markers exist,
/// or other errors when artifact creation fails.
pub fn run(paths: &InitPaths) -> Result<(), VaultError> {
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

    Ok(())
}

fn write_readme(path: &Path) -> Result<(), VaultError> {
    std::fs::write(path, README)?;
    Ok(())
}
