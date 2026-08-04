//! In-memory registry store fake.

use std::path::Path;
use std::sync::Mutex;

use crate::error::VaultError;
use crate::ports::RegistryStore;
use crate::registry::VaultRegistry;

/// In-memory registry store.
pub struct InMemoryRegistry {
    registry: Mutex<VaultRegistry>,
}

impl Default for InMemoryRegistry {
    fn default() -> Self {
        Self {
            registry: Mutex::new(VaultRegistry::default()),
        }
    }
}

impl RegistryStore for InMemoryRegistry {
    fn load(&self) -> Result<VaultRegistry, VaultError> {
        Ok(self
            .registry
            .lock()
            .map_err(|_| VaultError::TaskPanicked)?
            .clone())
    }

    fn save(&self, registry: &VaultRegistry) -> Result<(), VaultError> {
        *self.registry.lock().map_err(|_| VaultError::TaskPanicked)? = registry.clone();
        Ok(())
    }

    fn register(&self, root: &Path) -> Result<bool, VaultError> {
        let mut registry = self.registry.lock().map_err(|_| VaultError::TaskPanicked)?;
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if registry.vault.iter().any(|entry| entry.root == root) {
            return Ok(false);
        }
        registry.vault.push(crate::registry::VaultEntry {
            root,
            registered_at: chrono::Utc::now(),
            enabled: true,
        });
        Ok(true)
    }

    fn prune_stale(&self) -> Result<usize, VaultError> {
        let mut registry = self.registry.lock().map_err(|_| VaultError::TaskPanicked)?;
        let before = registry.vault.len();
        registry.vault.retain(|entry| entry.root.is_dir());
        Ok(before.saturating_sub(registry.vault.len()))
    }
}
