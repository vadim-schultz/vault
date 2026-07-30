//! Path resolution for vault initialization.

use std::path::{Path, PathBuf};

use crate::error::VaultError;

/// Default `.vault/` directory name relative to the worktree.
pub const VAULT_DIR: &str = ".vault";

/// Config filename inside `.vault/`.
pub const CONFIG_FILE: &str = "config.toml";

/// `SQLite` metadata database filename inside `.vault/`.
pub const META_DB: &str = "meta.db";

/// Bare git object store directory inside `.vault/`.
pub const GIT_DIR: &str = ".git";

/// Recovery guide filename inside `.vault/`.
pub const README_FILE: &str = "README";

/// Resolved paths for `vault init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPaths {
    /// Vault root (parent of `.vault/`).
    pub worktree: PathBuf,
    /// `.vault/` directory path.
    pub vault_dir: PathBuf,
}

impl InitPaths {
    /// Path to `.vault/config.toml`.
    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.vault_dir.join(CONFIG_FILE)
    }

    /// Path to `.vault/meta.db`.
    #[must_use]
    pub fn meta_db_path(&self) -> PathBuf {
        self.vault_dir.join(META_DB)
    }

    /// Path to `.vault/.git/`.
    #[must_use]
    pub fn git_dir_path(&self) -> PathBuf {
        self.vault_dir.join(GIT_DIR)
    }

    /// Path to `.vault/README`.
    #[must_use]
    pub fn readme_path(&self) -> PathBuf {
        self.vault_dir.join(README_FILE)
    }
}

/// Return whether a vault directory already contains init artifacts.
#[must_use]
pub fn is_initialized(vault_dir: &Path) -> bool {
    [CONFIG_FILE, META_DB, GIT_DIR, README_FILE]
        .iter()
        .any(|name| vault_dir.join(name).exists())
}

/// Resolve worktree and `.vault/` paths for initialization.
///
/// # Errors
///
/// Returns [`VaultError::InvalidVaultPath`] when `--vault-path` has no parent,
/// or [`VaultError::AlreadyInitialized`] when init markers are present.
pub fn resolve_init(vault_path: Option<PathBuf>) -> Result<InitPaths, VaultError> {
    let paths = match vault_path {
        None => {
            let worktree = std::env::current_dir()?;
            let vault_dir = worktree.join(VAULT_DIR);
            InitPaths {
                worktree,
                vault_dir,
            }
        }
        Some(vault_path) => {
            let vault_dir = vault_path;
            let worktree = vault_dir
                .parent()
                .ok_or_else(|| VaultError::InvalidVaultPath {
                    path: vault_dir.clone(),
                })?;
            InitPaths {
                worktree: worktree.to_path_buf(),
                vault_dir,
            }
        }
    };

    if is_initialized(&paths.vault_dir) {
        return Err(VaultError::AlreadyInitialized {
            path: paths.vault_dir,
        });
    }

    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn resolve_init_defaults_to_cwd_vault() {
        let dir = TempDir::new().expect("tempdir");
        let expected = dir.path().join(VAULT_DIR);

        std::env::set_current_dir(dir.path()).expect("chdir");
        let paths = resolve_init(None).expect("resolve");

        assert_eq!(paths.vault_dir, expected);
        assert_eq!(paths.worktree, dir.path());
    }

    #[test]
    fn resolve_init_rejects_existing_vault() {
        let dir = TempDir::new().expect("tempdir");
        let vault_dir = dir.path().join(VAULT_DIR);
        fs::create_dir_all(&vault_dir).expect("mkdir");
        fs::write(vault_dir.join(META_DB), b"").expect("touch meta.db");

        let err = resolve_init(Some(vault_dir)).expect_err("should fail");
        assert!(matches!(err, VaultError::AlreadyInitialized { .. }));
    }

    #[test]
    fn is_initialized_detects_readme() {
        let dir = TempDir::new().expect("tempdir");
        let vault_dir = dir.path().join(VAULT_DIR);
        fs::create_dir_all(&vault_dir).expect("mkdir");
        fs::write(vault_dir.join(README_FILE), b"partial").expect("touch README");

        assert!(is_initialized(&vault_dir));
    }
}
