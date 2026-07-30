//! Vault configuration (`config.toml`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::VaultError;

/// Vault configuration persisted in `.vault/config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Directories to watch for changes, relative to the vault root.
    pub watch_roots: Vec<String>,
    /// Glob patterns to ignore when watching.
    pub ignore: Vec<String>,
}

impl VaultConfig {
    /// Return the default configuration for a new vault.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            watch_roots: vec![".".to_string()],
            ignore: vec![
                ".vault/**".to_string(),
                "**/*.swp".to_string(),
                "**/*~".to_string(),
                "**/.#*".to_string(),
                "**/#*#".to_string(),
                "**/*.pdf".to_string(),
                "**/*.zip".to_string(),
            ],
        }
    }

    /// Serialize and write this config to `path`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] on serialization or I/O failure.
    pub fn write_to(&self, path: &Path) -> Result<(), VaultError> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
