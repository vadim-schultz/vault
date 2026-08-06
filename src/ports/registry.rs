#![allow(clippy::missing_errors_doc)]

//! Global vault registry port.

use std::path::{Path, PathBuf};

use crate::error::VaultError;
use crate::registry::VaultRegistry;

/// Global vault registry persistence.
pub trait RegistryStore: Send + Sync {
    /// Load the registry from disk.
    fn load(&self) -> Result<VaultRegistry, VaultError>;

    /// Save the registry atomically.
    fn save(&self, registry: &VaultRegistry) -> Result<(), VaultError>;

    /// Register `root` when not already present.
    fn register(&self, root: &Path) -> Result<bool, VaultError>;

    /// Remove vault entries whose roots no longer exist, returning the removed roots.
    fn prune_stale(&self) -> Result<Vec<PathBuf>, VaultError>;
}
