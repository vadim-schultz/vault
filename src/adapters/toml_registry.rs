//! TOML-backed [`RegistryStore`] adapter.

use std::path::Path;

use crate::error::VaultError;
use crate::ports::RegistryStore;
use crate::registry::VaultRegistry;

/// Global registry backed by `registry.toml`.
pub struct TomlRegistry;

impl RegistryStore for TomlRegistry {
    fn load(&self) -> Result<VaultRegistry, VaultError> {
        VaultRegistry::load()
    }

    fn save(&self, registry: &VaultRegistry) -> Result<(), VaultError> {
        registry.save()
    }

    fn register(&self, root: &Path) -> Result<bool, VaultError> {
        let mut registry = VaultRegistry::load()?;
        registry.register(root)
    }

    fn prune_stale(&self) -> Result<usize, VaultError> {
        let mut registry = VaultRegistry::load()?;
        registry.prune_stale()
    }
}
