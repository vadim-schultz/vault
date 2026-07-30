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
}
