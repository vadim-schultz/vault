//! Typed errors for vault library operations.

use std::path::PathBuf;

use crate::paths::META_DB;

/// Errors returned by vault library operations.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// Partial vault artifacts exist, and at least one data-bearing marker
    /// (`.git`/`meta.db`) is among the missing ones, so automatic repair was
    /// refused.
    #[error(
        "incomplete vault at {path} (found: {found}; missing: {missing}){}",
        reindex_hint(missing)
    )]
    PartialVault {
        /// Path to the partial `.vault/` directory.
        path: PathBuf,
        /// Human-readable list of present markers.
        found: String,
        /// Human-readable list of missing markers.
        missing: String,
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

    /// No snapshot exists at or before the requested timestamp.
    #[error("no snapshot at or before {at}")]
    NoSnapshotAt {
        /// The requested timestamp (UTC RFC3339).
        at: String,
    },

    /// The path did not exist (or was deleted) in the resolved snapshot.
    #[error("{path} was not tracked at {at}")]
    PathNotTrackedAt {
        /// The requested path.
        path: String,
        /// The requested timestamp (UTC RFC3339).
        at: String,
    },

    /// `meta.db` contained a value outside the schema's expected domain.
    #[error("corrupt metadata index: {detail}")]
    CorruptMetaIndex {
        /// Human-readable description of the unexpected value.
        detail: String,
    },

    /// A `--at`/`--to` value did not match any accepted date format.
    #[error("invalid date '{input}' (expected YYYY-MM-DD, YYYY-MM-DD HH:MM, or RFC3339)")]
    InvalidDate {
        /// The raw input string.
        input: String,
    },

    /// A queued background task was not found.
    #[error("queued task not found: {id}")]
    TaskNotFound {
        /// Task identifier.
        id: u64,
    },

    /// `vault reindex` found a commit with more than one parent while walking `.git`'s history.
    /// Vault's own writes only ever produce single-parent commits, so this means `.vault/.git`
    /// was mutated by something other than vault itself; refuse rather than silently picking a
    /// parent and hiding history.
    #[error("commit {commit_sha} has more than one parent — .vault/.git history is not linear, refusing to reindex")]
    NonLinearHistory {
        /// The offending commit's hex SHA.
        commit_sha: String,
    },

    /// `vault reindex` refused to overwrite an existing, populated `meta.db` without `--force`.
    #[error("meta.db at {path} already has {snapshot_count} snapshot(s) — pass --force to rebuild it from .git history")]
    MetaDbNotEmpty {
        /// Path to the existing `meta.db`.
        path: PathBuf,
        /// Number of snapshot rows already present.
        snapshot_count: i64,
    },
}

/// Extra hint appended to [`VaultError::PartialVault`]'s message when `missing` names only
/// `meta.db` — that's the one data-bearing marker with a safe, automated fix (`vault reindex`,
/// see `app::reindex`); `.git` missing (alone or alongside `meta.db`) still has none.
fn reindex_hint(missing: &str) -> &'static str {
    if missing == META_DB {
        " — run `vault reindex` to rebuild it from .git history"
    } else {
        ""
    }
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
    fn partial_vault_hints_reindex_when_only_meta_db_is_missing() {
        let err = VaultError::PartialVault {
            path: PathBuf::from("/vault/.vault"),
            found: "config.toml, .git, README".to_string(),
            missing: "meta.db".to_string(),
        };
        assert!(err.to_string().contains("run `vault reindex`"));
    }

    #[test]
    fn partial_vault_has_no_reindex_hint_when_git_is_also_missing() {
        let err = VaultError::PartialVault {
            path: PathBuf::from("/vault/.vault"),
            found: "config.toml, README".to_string(),
            missing: ".git, meta.db".to_string(),
        };
        assert!(!err.to_string().contains("vault reindex"));
    }

    #[test]
    fn invalid_glob_is_not_io_error() {
        let err = VaultError::InvalidGlob {
            pattern: "[unclosed".to_string(),
        };
        assert!(matches!(err, VaultError::InvalidGlob { .. }));
    }

    #[test]
    fn new_variants_construct() {
        assert!(matches!(
            VaultError::NoSnapshotAt {
                at: "2026-01-01".to_string()
            },
            VaultError::NoSnapshotAt { .. }
        ));
        assert!(matches!(
            VaultError::PathNotTrackedAt {
                path: "a.md".to_string(),
                at: "2026-01-01".to_string()
            },
            VaultError::PathNotTrackedAt { .. }
        ));
        assert!(matches!(
            VaultError::CorruptMetaIndex {
                detail: "unknown event_type".to_string()
            },
            VaultError::CorruptMetaIndex { .. }
        ));
        assert!(matches!(
            VaultError::InvalidDate {
                input: "bad-date".to_string()
            },
            VaultError::InvalidDate { .. }
        ));
    }
}
