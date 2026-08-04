//! Vault configuration (`config.toml`).

mod gc;
mod watcher;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::VaultError;

pub use gc::GcConfig;
pub use watcher::WatcherConfig;

/// Glob patterns ignored by default in a freshly initialized vault.
const DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    ".vault/**",
    ".git/**",
    "**/*.swp",
    "**/*~",
    "**/.#*",
    "**/#*#",
    "**/*.pdf",
    "**/*.zip",
];

/// Vault configuration persisted in `.vault/config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Directories to watch for changes, relative to the vault root.
    pub watch_roots: Vec<String>,
    /// Glob patterns to ignore when watching.
    pub ignore: Vec<String>,
    /// Watcher tuning options.
    #[serde(default)]
    pub watcher: WatcherConfig,
    /// Git housekeeping thresholds.
    #[serde(default)]
    pub gc: GcConfig,
}

impl VaultConfig {
    /// Return the default configuration for a new vault.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            watch_roots: vec![".".to_string()],
            ignore: DEFAULT_IGNORE_PATTERNS
                .iter()
                .map(ToString::to_string)
                .collect(),
            watcher: WatcherConfig::default(),
            gc: GcConfig::default(),
        }
    }

    /// Load configuration from `path`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] on I/O or parse failure.
    pub fn load(path: &Path) -> Result<Self, VaultError> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
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

    /// Append an ignore pattern when it is not already present.
    ///
    /// Returns `true` when the pattern was added.
    #[must_use]
    pub fn add_ignore(&mut self, pattern: &str) -> bool {
        if self.ignore.iter().any(|p| p == pattern) {
            return false;
        }
        self.ignore.push(pattern.to_string());
        true
    }
}
