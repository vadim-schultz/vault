//! Vault configuration (`config.toml`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::VaultError;

/// Watcher settings in `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherConfig {
    /// Debounce window in milliseconds before committing a snapshot.
    #[serde(default = "WatcherConfig::default_debounce_ms")]
    pub debounce_ms: u64,
    /// Maximum file size in bytes to snapshot.
    #[serde(default = "WatcherConfig::default_max_file_bytes")]
    pub max_file_bytes: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: Self::DEFAULT_DEBOUNCE_MS,
            max_file_bytes: Self::DEFAULT_MAX_FILE_BYTES,
        }
    }
}

impl WatcherConfig {
    /// Default debounce interval in milliseconds.
    pub const DEFAULT_DEBOUNCE_MS: u64 = 2000;

    /// Default maximum file size in bytes to snapshot.
    pub const DEFAULT_MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

    const fn default_debounce_ms() -> u64 {
        Self::DEFAULT_DEBOUNCE_MS
    }

    const fn default_max_file_bytes() -> u64 {
        Self::DEFAULT_MAX_FILE_BYTES
    }
}

/// Git housekeeping thresholds in `config.toml` (`[gc]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcConfig {
    /// Loose object count above which a repack is due (matches `gc.auto`, default 6700).
    #[serde(default = "GcConfig::default_loose_object_limit")]
    pub loose_object_limit: usize,
    /// Packfile count above which a repack is due (matches `gc.autopacklimit`, default 50).
    #[serde(default = "GcConfig::default_pack_limit")]
    pub pack_limit: usize,
    /// Seconds since the last repack after which a repack is due (weekly cadence).
    #[serde(default = "GcConfig::default_max_age_secs")]
    pub max_age_secs: u64,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            loose_object_limit: Self::DEFAULT_LOOSE_OBJECT_LIMIT,
            pack_limit: Self::DEFAULT_PACK_LIMIT,
            max_age_secs: Self::DEFAULT_MAX_AGE_SECS,
        }
    }
}

impl GcConfig {
    /// Default loose-object threshold (`gc.auto`).
    pub const DEFAULT_LOOSE_OBJECT_LIMIT: usize = 6700;

    /// Default packfile threshold (`gc.autopacklimit`).
    pub const DEFAULT_PACK_LIMIT: usize = 50;

    /// Default maximum seconds between repacks (7 days).
    pub const DEFAULT_MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;

    const fn default_loose_object_limit() -> usize {
        Self::DEFAULT_LOOSE_OBJECT_LIMIT
    }

    const fn default_pack_limit() -> usize {
        Self::DEFAULT_PACK_LIMIT
    }

    const fn default_max_age_secs() -> u64 {
        Self::DEFAULT_MAX_AGE_SECS
    }

    /// Return the configured max age as a [`std::time::Duration`].
    #[must_use]
    pub fn max_age_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.max_age_secs)
    }
}

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
