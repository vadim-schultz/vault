//! Global vault registry (`registry.toml`).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs4::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::VaultError;
use crate::paths::{registry_lock_path, registry_path};

/// Supported `registry.toml` format version.
pub const REGISTRY_VERSION: u32 = 1;

/// Global registry of vault roots watched by the singleton daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultRegistry {
    /// Registry format version.
    pub version: u32,
    /// Registered vaults.
    #[serde(default)]
    pub vault: Vec<VaultEntry>,
}

/// One registered vault root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultEntry {
    /// Vault worktree root (parent of `.vault/`).
    pub root: PathBuf,
    /// When the vault was registered.
    pub registered_at: DateTime<Utc>,
    /// Whether the daemon should watch this vault.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

const fn default_enabled() -> bool {
    true
}

impl Default for VaultRegistry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            vault: Vec::new(),
        }
    }
}

impl VaultRegistry {
    /// Load the registry from disk, returning an empty registry when missing.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] on parse failure or unsupported version.
    pub fn load() -> Result<Self, VaultError> {
        let path = registry_path()?;
        if !path.is_file() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)?;
        let registry: Self = toml::from_str(&contents)?;
        registry.validate_version()?;
        Ok(registry)
    }

    /// Save the registry atomically under `registry.lock`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] on I/O or serialization failure.
    pub fn save(&self) -> Result<(), VaultError> {
        let path = registry_path()?;
        let lock_path = registry_lock_path()?;
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;

        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        FileExt::lock(&lock_file)?;

        let tmp = path.with_extension("toml.tmp");
        let contents = toml::to_string_pretty(self)?;
        {
            let mut file = File::create(&tmp)?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(tmp, &path)?;

        FileExt::unlock(&lock_file)?;
        Ok(())
    }

    /// Register `root` when not already present.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the registry cannot be saved.
    pub fn register(&mut self, root: &Path) -> Result<bool, VaultError> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if self.vault.iter().any(|entry| entry.root == root) {
            return Ok(false);
        }
        self.vault.push(VaultEntry {
            root,
            registered_at: Utc::now(),
            enabled: true,
        });
        self.save()?;
        Ok(true)
    }

    /// Remove vault entries whose roots no longer exist on disk.
    ///
    /// Returns the roots that were removed.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the registry cannot be saved.
    pub fn prune_stale(&mut self) -> Result<Vec<PathBuf>, VaultError> {
        let (kept, removed): (Vec<_>, Vec<_>) =
            self.vault.drain(..).partition(|entry| entry.root.is_dir());
        self.vault = kept;
        let removed: Vec<PathBuf> = removed.into_iter().map(|entry| entry.root).collect();
        if !removed.is_empty() {
            self.save()?;
        }
        Ok(removed)
    }

    /// Return enabled vault roots, longest path first (deepest match wins).
    #[must_use]
    pub fn enabled_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<_> = self
            .vault
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.root.clone())
            .collect();
        roots.sort_by_key(|b| std::cmp::Reverse(b.components().count()));
        roots
    }

    fn validate_version(&self) -> Result<(), VaultError> {
        if self.version != REGISTRY_VERSION {
            return Err(VaultError::UnsupportedRegistryVersion {
                found: self.version,
                expected: REGISTRY_VERSION,
            });
        }
        Ok(())
    }
}

/// Register `root` in the global registry.
///
/// # Errors
///
/// Returns [`VaultError`] when load or save fails.
pub fn register(root: &Path) -> Result<bool, VaultError> {
    let mut registry = VaultRegistry::load()?;
    registry.register(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn with_state<F: FnOnce()>(dir: &TempDir, f: F) {
        let _guard = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        std::env::set_var(crate::paths::STATE_DIR_ENV, dir.path());
        f();
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = TempDir::new().expect("tempdir");
        with_state(&dir, || {
            let mut registry = VaultRegistry::default();
            registry
                .register(dir.path())
                .expect("register should succeed on first call");
            let loaded = VaultRegistry::load().expect("load");
            assert_eq!(loaded.vault.len(), 1);
            assert_eq!(loaded.vault[0].root, dir.path().canonicalize().unwrap());
        });
    }

    #[test]
    fn register_dedupes() {
        let dir = TempDir::new().expect("tempdir");
        with_state(&dir, || {
            let mut registry = VaultRegistry::default();
            assert!(registry.register(dir.path()).expect("first"));
            assert!(!registry.register(dir.path()).expect("second"));
            assert_eq!(registry.vault.len(), 1);
        });
    }

    #[test]
    fn prune_stale_removes_missing_roots() {
        let dir = TempDir::new().expect("tempdir");
        with_state(&dir, || {
            let missing = dir.path().join("gone");
            let mut registry = VaultRegistry::default();
            registry.vault.push(VaultEntry {
                root: missing.clone(),
                registered_at: Utc::now(),
                enabled: true,
            });
            registry.save().expect("save");
            let removed = registry.prune_stale().expect("prune");
            assert_eq!(removed, vec![missing]);
            assert!(registry.vault.is_empty());
        });
    }

    #[test]
    fn prune_stale_keeps_present_roots() {
        let dir = TempDir::new().expect("tempdir");
        with_state(&dir, || {
            let mut registry = VaultRegistry::default();
            registry
                .register(dir.path())
                .expect("register should succeed");
            let removed = registry.prune_stale().expect("prune");
            assert!(removed.is_empty());
            assert_eq!(registry.vault.len(), 1);
        });
    }

    #[test]
    fn rejects_unsupported_version() {
        let dir = TempDir::new().expect("tempdir");
        with_state(&dir, || {
            let path = registry_path().expect("path");
            fs::write(path, "version = 99\nvault = []\n").expect("write");
            let err = VaultRegistry::load().expect_err("version");
            assert!(matches!(err, VaultError::UnsupportedRegistryVersion { .. }));
        });
    }
}
