//! Route filesystem events to the owning vault.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::VaultConfig;
use crate::domain::{vault_state, RelPath, VaultLayout, VaultState};
use crate::error::VaultError;
use crate::ignore::IgnoreMatcher;
use crate::paths::{self};
use crate::registry::VaultRegistry;

/// One vault the watcher is actively monitoring.
#[derive(Debug, Clone)]
pub struct WatchedVault {
    /// Vault worktree root.
    pub root: PathBuf,
    /// Resolved vault layout.
    pub layout: VaultLayout,
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
    /// Build a router from the global registry, skipping broken vaults.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when vault configs cannot be loaded.
    pub fn from_registry(registry: &VaultRegistry) -> Result<Self, VaultError> {
        let mut vaults = Vec::new();
        for root in registry.enabled_roots() {
            match load_vault(&root) {
                Ok(vault) => vaults.push(vault),
                Err(err) => {
                    let _ = crate::daemon::append_log(&format!(
                        "skipping vault {}: {err}",
                        root.display()
                    ));
                }
            }
        }
        Ok(Self { vaults })
    }

    /// Return all watched vault roots.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        self.vaults.iter().map(|v| v.root.clone()).collect()
    }

    /// Return the minimum debounce across vaults (milliseconds).
    #[must_use]
    pub fn min_debounce_ms(&self) -> u64 {
        self.vaults
            .iter()
            .map(|v| v.config.watcher.debounce_ms)
            .min()
            .unwrap_or(crate::config::WatcherConfig::DEFAULT_DEBOUNCE_MS)
    }

    /// Route absolute paths to vault batches with pre-filtered relative paths.
    #[must_use]
    pub fn route(&self, abs_paths: Vec<PathBuf>) -> Vec<(WatchedVault, Vec<RelPath>)> {
        let mut grouped: HashMap<PathBuf, Vec<RelPath>> = HashMap::new();
        for abs in abs_paths {
            let Some(vault) = self.vault_for(&abs) else {
                continue;
            };
            let Ok(rel) = RelPath::from_worktree(&vault.root, &abs) else {
                continue;
            };
            if vault.ignore.is_ignored(&rel) {
                continue;
            }
            grouped.entry(vault.root.clone()).or_default().push(rel);
        }
        grouped
            .into_iter()
            .filter_map(|(root, mut paths)| {
                paths.sort();
                paths.dedup();
                let vault = self.vaults.iter().find(|v| v.root == root)?.clone();
                Some((vault, paths))
            })
            .collect()
    }

    fn vault_for(&self, abs_path: &Path) -> Option<&WatchedVault> {
        let canonical = abs_path
            .canonicalize()
            .unwrap_or_else(|_| abs_path.to_path_buf());
        self.vaults
            .iter()
            .filter(|vault| canonical.starts_with(&vault.root))
            .max_by_key(|vault| vault.root.components().count())
    }
}

fn load_vault(root: &Path) -> Result<WatchedVault, VaultError> {
    let vault_dir = root.join(paths::VAULT_DIR);
    if !matches!(vault_state(&vault_dir), VaultState::Ready) {
        return Err(VaultError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "vault not ready",
        )));
    }
    let layout = VaultLayout::from_worktree(root.to_path_buf());
    let config = VaultConfig::load(&layout.config_path())?;
    let ignore = IgnoreMatcher::from_config(&config)?;
    Ok(WatchedVault {
        root: root.to_path_buf(),
        layout,
        config,
        ignore,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use crate::domain::VaultLayout;
    use crate::registry::VaultRegistry;
    use crate::storage;

    fn setup_vault(dir: &TempDir) -> PathBuf {
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git");
        storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite");
        VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("config");
        std::fs::write(layout.readme_path(), b"test").expect("readme");
        layout.worktree
    }

    #[test]
    fn nested_vault_uses_deepest_root() {
        let outer = TempDir::new().expect("tempdir");
        let inner = outer.path().join("nested");
        fs::create_dir_all(&inner).expect("mkdir");
        let _outer_root = setup_vault(&outer);

        let inner_layout = VaultLayout::from_worktree(inner.clone());
        std::fs::create_dir_all(&inner_layout.vault_dir).expect("mkdir");
        storage::git::init(&inner_layout.git_dir_path(), &inner_layout.worktree).expect("git");
        storage::sqlite::init_meta_db(&inner_layout.meta_db_path()).expect("sqlite");
        VaultConfig::defaults()
            .write_to(&inner_layout.config_path())
            .expect("config");
        std::fs::write(inner_layout.readme_path(), b"test").expect("readme");

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
        let batches = router.route(vec![file]);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0.root, inner);
    }

    #[test]
    fn corrupt_vault_config_skipped() {
        let good = TempDir::new().expect("good");
        let bad = TempDir::new().expect("bad");
        setup_vault(&good);
        let bad_layout = VaultLayout::from_worktree(bad.path().to_path_buf());
        std::fs::create_dir_all(&bad_layout.vault_dir).expect("mkdir");
        storage::git::init(&bad_layout.git_dir_path(), &bad_layout.worktree).expect("git");
        storage::sqlite::init_meta_db(&bad_layout.meta_db_path()).expect("sqlite");
        std::fs::write(bad_layout.config_path(), "not valid toml [[[").expect("bad config");
        fs::write(bad_layout.readme_path(), b"x").expect("readme");

        let mut registry = VaultRegistry::default();
        for root in [good.path().to_path_buf(), bad.path().to_path_buf()] {
            registry.vault.push(crate::registry::VaultEntry {
                root,
                registered_at: chrono::Utc::now(),
                enabled: true,
            });
        }
        let router = Router::from_registry(&registry).expect("router");
        assert_eq!(router.roots().len(), 1);
    }

    #[test]
    fn ignored_path_never_reaches_object_store() {
        let dir = TempDir::new().expect("tempdir");
        setup_vault(&dir);
        fs::write(dir.path().join("notes.md"), b"n").expect("notes");
        fs::write(dir.path().join("notes.md.swp"), b"swap").expect("swp");

        let mut registry = VaultRegistry::default();
        registry.vault.push(crate::registry::VaultEntry {
            root: dir.path().to_path_buf(),
            registered_at: chrono::Utc::now(),
            enabled: true,
        });

        let router = Router::from_registry(&registry).expect("router");
        let batches = router.route(vec![
            dir.path().join("notes.md"),
            dir.path().join("notes.md.swp"),
        ]);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].1.len(), 1);
        assert_eq!(batches[0].1[0].as_str(), "notes.md");
    }
}
