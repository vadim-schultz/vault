//! Vault layout and initialization state.

use std::path::{Path, PathBuf};

use crate::paths::{CONFIG_FILE, GIT_DIR, META_DB, README_FILE, VAULT_DIR};

/// Resolved paths for a vault worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultLayout {
    /// Vault root (parent of `.vault/`).
    pub worktree: PathBuf,
    /// `.vault/` directory path.
    pub vault_dir: PathBuf,
}

impl VaultLayout {
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

    /// Build layout from worktree (`.vault/` is `worktree/.vault`).
    #[must_use]
    pub fn from_worktree(worktree: PathBuf) -> Self {
        let vault_dir = worktree.join(VAULT_DIR);
        Self {
            worktree,
            vault_dir,
        }
    }

    /// Build layout when `.vault/` path is known.
    #[must_use]
    pub fn from_vault_dir(vault_dir: PathBuf, worktree: PathBuf) -> Self {
        Self {
            worktree,
            vault_dir,
        }
    }
}

/// Initialization state of a `.vault/` directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultState {
    /// No vault artifacts present.
    Absent,
    /// Some but not all init markers exist.
    Partial(Vec<&'static str>),
    /// All init markers present.
    Ready,
}

/// Return the initialization state of `vault_dir`.
#[must_use]
pub fn vault_state(vault_dir: &Path) -> VaultState {
    const MARKERS: &[&str] = &[CONFIG_FILE, META_DB, GIT_DIR, README_FILE];
    let present: Vec<&str> = MARKERS
        .iter()
        .filter(|name| vault_dir.join(name).exists())
        .copied()
        .collect();
    match present.len() {
        0 => VaultState::Absent,
        n if n == MARKERS.len() => VaultState::Ready,
        _ => VaultState::Partial(present),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn lone_readme_is_partial_not_ready() {
        let dir = TempDir::new().expect("tempdir");
        let vault_dir = dir.path().join(VAULT_DIR);
        fs::create_dir_all(&vault_dir).expect("mkdir");
        fs::write(vault_dir.join(README_FILE), b"x").expect("write");

        assert_eq!(
            vault_state(&vault_dir),
            VaultState::Partial(vec![README_FILE])
        );
    }

    #[test]
    fn absent_when_empty() {
        let dir = TempDir::new().expect("tempdir");
        assert_eq!(vault_state(&dir.path().join(VAULT_DIR)), VaultState::Absent);
    }
}
