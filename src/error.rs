//! Typed errors for vault library operations.

use std::path::PathBuf;

/// Errors returned by vault library operations.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// A vault already exists at the target path.
    #[error("vault already initialized at {path}")]
    AlreadyInitialized {
        /// Path to the existing `.vault/` directory.
        path: PathBuf,
    },

    /// `--vault-path` does not have a parent directory.
    #[error("vault directory missing parent: {path}")]
    InvalidVaultPath {
        /// The invalid vault path.
        path: PathBuf,
    },

    /// No `.vault/` directory was found.
    #[error("no vault found starting from {start}")]
    VaultNotFound {
        /// Directory where discovery began.
        start: PathBuf,
    },

    /// The singleton daemon is already running.
    #[error("vault daemon already running (pid {pid})")]
    DaemonAlreadyRunning {
        /// Process id holding the daemon lock.
        pid: u32,
    },

    /// The singleton daemon is not running.
    #[error("vault daemon is not running")]
    DaemonNotRunning,

    /// Registry file has an unsupported version.
    #[error("unsupported registry version {found}, expected {expected}")]
    UnsupportedRegistryVersion {
        /// Version found on disk.
        found: u32,
        /// Version this binary supports.
        expected: u32,
    },

    /// Filesystem notification error.
    #[error("filesystem watcher error: {0}")]
    Notify(String),

    /// Service manager operation failed.
    #[error("service manager error: {0}")]
    Service(String),

    /// I/O failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Git storage operation failed.
    #[error("git storage error: {0}")]
    Git(String),

    /// `SQLite` operation failed.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    /// TOML serialization failed.
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),

    /// TOML deserialization failed.
    #[error(transparent)]
    TomlDeserialize(#[from] toml::de::Error),

    /// JSON serialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
