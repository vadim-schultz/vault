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

    /// Partial vault artifacts exist.
    #[error("incomplete vault at {path} (found: {found})")]
    PartialVault {
        /// Path to the partial `.vault/` directory.
        path: PathBuf,
        /// Human-readable list of present markers.
        found: String,
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

    /// Path is not under the vault worktree.
    #[error("path outside worktree: {path}")]
    PathOutsideWorktree {
        /// The offending path.
        path: PathBuf,
    },

    /// Path component is not valid UTF-8.
    #[error("non-UTF-8 path: {path:?}")]
    NonUtf8Path {
        /// The offending path.
        path: PathBuf,
    },

    /// Invalid glob pattern.
    #[error("invalid glob pattern: {pattern}")]
    InvalidGlob {
        /// The invalid pattern.
        pattern: String,
    },

    /// Platform state directory could not be resolved.
    #[error("could not resolve user data directory")]
    StateDirUnresolved,

    /// Advisory lock is held by another process.
    #[error("daemon lock held")]
    LockHeld,

    /// A spawned task panicked.
    #[error("background task panicked")]
    TaskPanicked,

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
    #[error("filesystem watcher error")]
    Notify(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Service manager operation failed.
    #[error("service manager error")]
    Service(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// I/O failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Git storage operation failed.
    #[error("git storage error")]
    Git(#[source] Box<dyn std::error::Error + Send + Sync>),

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

impl VaultError {
    /// Wrap a git error.
    pub fn git(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Git(Box::new(err))
    }

    /// Wrap a notify error.
    pub fn notify(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Notify(Box::new(err))
    }

    /// Wrap a service error.
    pub fn service(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Service(Box::new(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_glob_is_not_io_error() {
        let err = VaultError::InvalidGlob {
            pattern: "[unclosed".to_string(),
        };
        assert!(matches!(err, VaultError::InvalidGlob { .. }));
    }
}
