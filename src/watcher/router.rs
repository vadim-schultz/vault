//! Route filesystem events to the owning vault.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::VaultConfig;
use crate::error::VaultError;
use crate::ignore::IgnoreMatcher;
use crate::paths::{self, VaultPaths};
use crate::registry::VaultRegistry;

/// One vault the watcher is actively monitoring.
#[derive(Debug, Clone)]
pub struct WatchedVault {
    /// Vault worktree root.
    pub root: PathBuf,
    /// Resolved vault paths.
    pub paths: VaultPaths,
    /// Loaded configuration.
    pub config: VaultConfig,
    /// Compiled ignore matcher.
    pub ignore: IgnoreMatcher,
}

/// Maps absolute paths to vaults (longest root wins).
#[derive(Debug, Default)]
pub struct Router {
    vaults: Vec<WatchedVault>,
}

impl Router {
    /// Build a router from the global registry.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when vault configs cannot be loaded.
    pub fn from_registry(registry: &VaultRegistry) -> Result<Self, VaultError> {
        let mut vaults = Vec::new();
        for root in registry.enabled_roots() {
            let vault_dir = root.join(paths::VAULT_DIR);
            if !paths::is_initialized(&vault_dir) {
                continue;
            }
            let vault_paths = VaultPaths {
                worktree: root.clone(),
                vault_dir,
            };
            let config = VaultConfig::load(&vault_paths.config_path())?;
            let ignore = IgnoreMatcher::from_config(&config)?;
            vaults.push(WatchedVault {
                root,
                paths: vault_paths,
                config,
                ignore,
            });
        }
        Ok(Self { vaults })
    }

    /// Return the vault owning `abs_path`, if any.
    #[must_use]
    pub fn vault_for(&self, abs_path: &Path) -> Option<&WatchedVault> {
        let canonical = abs_path
            .canonicalize()
            .unwrap_or_else(|_| abs_path.to_path_buf());
        self.vaults
            .iter()
            .filter(|vault| canonical.starts_with(&vault.root))
            .max_by_key(|vault| vault.root.components().count())
    }

    /// Return all watched vault roots.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        self.vaults.iter().map(|v| v.root.clone()).collect()
    }

    /// Return the vault with exactly `root`.
    #[must_use]
    pub fn vault_by_root(&self, root: &Path) -> Option<&WatchedVault> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        self.vaults.iter().find(|vault| vault.root == root)
    }

    /// Return the minimum debounce across vaults (milliseconds).
    #[must_use]
    pub fn min_debounce_ms(&self) -> u64 {
        self.vaults
            .iter()
            .map(|v| v.config.watcher.debounce_ms)
            .min()
            .unwrap_or(2000)
    }

    /// Group relative paths by vault root for snapshot commits.
    #[must_use]
    pub fn group_paths(&self, events: &[(PathBuf, PathBuf)]) -> HashMap<PathBuf, Vec<PathBuf>> {
        let mut grouped: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        for (abs, rel) in events {
            let Some(vault) = self.vault_for(abs) else {
                continue;
            };
            if vault.ignore.is_ignored(rel) {
                continue;
            }
            grouped
                .entry(vault.root.clone())
                .or_default()
                .push(rel.clone());
        }
        for paths in grouped.values_mut() {
            paths.sort();
            paths.dedup();
        }
        grouped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use crate::paths::InitPaths;
    use crate::registry::VaultRegistry;
    use crate::storage;

    fn setup_vault(dir: &TempDir) -> PathBuf {
        let paths = InitPaths {
            worktree: dir.path().to_path_buf(),
            vault_dir: dir.path().join(paths::VAULT_DIR),
        };
        std::fs::create_dir_all(&paths.vault_dir).expect("mkdir");
        storage::git::init(&paths.git_dir_path(), &paths.worktree).expect("git");
        storage::sqlite::init_meta_db(&paths.meta_db_path()).expect("sqlite");
        VaultConfig::defaults()
            .write_to(&paths.config_path())
            .expect("config");
        paths.worktree
    }

    #[test]
    fn nested_vault_uses_deepest_root() {
        let outer = TempDir::new().expect("tempdir");
        let inner = outer.path().join("nested");
        fs::create_dir_all(&inner).expect("mkdir");
        let _outer_root = setup_vault(&outer);

        let inner_paths = InitPaths {
            worktree: inner.clone(),
            vault_dir: inner.join(paths::VAULT_DIR),
        };
        std::fs::create_dir_all(&inner_paths.vault_dir).expect("mkdir");
        storage::git::init(&inner_paths.git_dir_path(), &inner_paths.worktree).expect("git");
        storage::sqlite::init_meta_db(&inner_paths.meta_db_path()).expect("sqlite");
        VaultConfig::defaults()
            .write_to(&inner_paths.config_path())
            .expect("config");

        let mut registry = VaultRegistry::default();
        registry.vault.push(crate::registry::VaultEntry {
            root: outer.path().to_path_buf(),
            registered_at: chrono::Utc::now(),
            enabled: true,
        });
        registry.vault.push(crate::registry::VaultEntry {
            root: inner.clone(),
            registered_at: chrono::Utc::now(),
            enabled: true,
        });

        let router = Router::from_registry(&registry).expect("router");
        let file = inner.join("notes.md");
        let vault = router.vault_for(&file).expect("vault");
        assert_eq!(vault.root, inner);
    }
}
