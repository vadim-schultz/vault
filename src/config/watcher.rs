//! Watcher settings in `config.toml`.

use serde::{Deserialize, Serialize};

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
