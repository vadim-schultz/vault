//! Registry pruning use-case.

use crate::error::VaultError;
use crate::ports::RegistryStore;

/// Remove vault entries whose roots no longer exist on disk.
///
/// # Errors
///
/// Returns [`VaultError`] when registry load or save fails.
pub fn prune(registry: &dyn RegistryStore) -> Result<usize, VaultError> {
    registry.prune_stale()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::TomlRegistry;
    use crate::paths::STATE_DIR_ENV;
    use crate::registry::{VaultEntry, VaultRegistry};
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn removes_missing_roots() {
        let _guard = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let dir = TempDir::new().expect("tempdir");
        std::env::set_var(STATE_DIR_ENV, dir.path());
        let store = TomlRegistry;
        let mut registry = VaultRegistry::default();
        registry.vault.push(VaultEntry {
            root: std::path::PathBuf::from("/nonexistent/vault/root"),
            registered_at: Utc::now(),
            enabled: true,
        });
        store.save(&registry).expect("save");
        let removed = prune(&store).expect("prune");
        assert_eq!(removed, 1);
        assert!(store.load().expect("load").vault.is_empty());
        std::env::remove_var(STATE_DIR_ENV);
    }
}
