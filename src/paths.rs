//! Path resolution for vault initialization and global state.

use std::path::{Path, PathBuf};

use crate::domain::{vault_state, VaultLayout, VaultState};
use crate::error::VaultError;

/// Default `.vault/` directory name relative to the worktree.
pub const VAULT_DIR: &str = ".vault";

/// Config filename inside `.vault/`.
pub const CONFIG_FILE: &str = "config.toml";

/// `SQLite` metadata database filename inside `.vault/`.
pub const META_DB: &str = "meta.db";

/// Bare git object store directory inside `.vault/`.
pub const GIT_DIR: &str = ".git";

/// Recovery guide filename inside `.vault/`.
pub const README_FILE: &str = "README";

/// Environment variable overriding the global state directory.
pub const STATE_DIR_ENV: &str = "VAULT_STATE_DIR";

/// Environment variable to skip service install/start during init.
pub const NO_SERVICE_ENV: &str = "VAULT_NO_SERVICE";

/// Global registry filename.
pub const REGISTRY_FILE: &str = "registry.toml";

/// Lock file for atomic registry writes.
pub const REGISTRY_LOCK: &str = "registry.lock";

/// Advisory lock file for the singleton daemon.
pub const DAEMON_LOCK: &str = "daemon.lock";

/// Daemon heartbeat JSON file.
pub const DAEMON_HEARTBEAT: &str = "daemon.json";

/// Daemon log file.
pub const DAEMON_LOG: &str = "daemon.log";

/// Serialize unit tests that override [`STATE_DIR_ENV`].
#[cfg(test)]
pub static STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Return whether service install/start should be skipped.
#[must_use]
pub fn skip_service() -> bool {
    std::env::var(NO_SERVICE_ENV)
        .is_ok_and(|v| !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"))
}

/// Return the global vault state directory path.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the platform data directory cannot be resolved.
pub fn state_dir() -> Result<PathBuf, VaultError> {
    state_dir_from_env().map_or_else(default_state_dir, Ok)
}

/// Create the global state directory when missing and return its path.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the path cannot be resolved or created.
pub fn ensure_state_dir() -> Result<PathBuf, VaultError> {
    let path = state_dir()?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Return `VAULT_STATE_DIR` when set to a non-empty path.
#[must_use]
fn state_dir_from_env() -> Option<PathBuf> {
    let dir = std::env::var(STATE_DIR_ENV).ok()?;
    if dir.is_empty() {
        return None;
    }
    Some(PathBuf::from(dir))
}

/// Return the default per-user state directory for this platform.
fn default_state_dir() -> Result<PathBuf, VaultError> {
    directories::ProjectDirs::from("", "", "vault")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or(VaultError::StateDirUnresolved)
}

/// Return the path to `registry.toml`.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the state directory cannot be resolved.
pub fn registry_path() -> Result<PathBuf, VaultError> {
    Ok(state_dir()?.join(REGISTRY_FILE))
}

/// Return the path to `registry.lock`.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the state directory cannot be resolved.
pub fn registry_lock_path() -> Result<PathBuf, VaultError> {
    Ok(state_dir()?.join(REGISTRY_LOCK))
}

/// Return the path to `daemon.lock`.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the state directory cannot be resolved.
pub fn daemon_lock_path() -> Result<PathBuf, VaultError> {
    Ok(state_dir()?.join(DAEMON_LOCK))
}

/// Return the path to `daemon.json`.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the state directory cannot be resolved.
pub fn daemon_heartbeat_path() -> Result<PathBuf, VaultError> {
    Ok(state_dir()?.join(DAEMON_HEARTBEAT))
}

/// Return the path to `daemon.log`.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the state directory cannot be resolved.
pub fn daemon_log_path() -> Result<PathBuf, VaultError> {
    Ok(state_dir()?.join(DAEMON_LOG))
}

/// Resolve worktree and `.vault/` paths for initialization.
///
/// # Errors
///
/// Returns [`VaultError::InvalidVaultPath`] when `--vault-path` has no parent,
/// or [`VaultError::AlreadyInitialized`] when init markers are present.
pub fn resolve_init(vault_path: Option<PathBuf>) -> Result<VaultLayout, VaultError> {
    let layout = resolve_layout(vault_path)?;
    match vault_state(&layout.vault_dir) {
        VaultState::Absent => Ok(layout),
        VaultState::Ready => Err(VaultError::AlreadyInitialized {
            path: layout.vault_dir,
        }),
        VaultState::Partial(found) => Err(VaultError::PartialVault {
            path: layout.vault_dir,
            found: found.join(", "),
        }),
    }
}

/// Resolve paths for an existing vault.
///
/// # Errors
///
/// Returns [`VaultError::VaultNotFound`] when no `.vault/` exists.
pub fn resolve_vault(vault_path: Option<PathBuf>) -> Result<VaultLayout, VaultError> {
    let layout = resolve_layout(vault_path)?;
    match vault_state(&layout.vault_dir) {
        VaultState::Ready => Ok(layout),
        VaultState::Absent => Err(VaultError::VaultNotFound {
            start: layout.worktree,
        }),
        VaultState::Partial(found) => Err(VaultError::PartialVault {
            path: layout.vault_dir,
            found: found.join(", "),
        }),
    }
}

fn resolve_layout(vault_path: Option<PathBuf>) -> Result<VaultLayout, VaultError> {
    match vault_path {
        None => {
            let worktree = std::env::current_dir()?;
            Ok(VaultLayout::from_worktree(worktree))
        }
        Some(vault_path) => {
            let vault_dir = vault_path;
            let worktree = vault_dir
                .parent()
                .ok_or_else(|| VaultError::InvalidVaultPath {
                    path: vault_dir.clone(),
                })?
                .to_path_buf();
            Ok(VaultLayout::from_vault_dir(vault_dir, worktree))
        }
    }
}

/// Return whether a vault directory is fully initialized.
#[must_use]
pub fn is_initialized(vault_dir: &Path) -> bool {
    matches!(vault_state(vault_dir), VaultState::Ready)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn resolve_init_defaults_to_cwd_vault() {
        let dir = TempDir::new().expect("tempdir");
        let restore = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");
        let expected = dir.path().join(VAULT_DIR);

        let layout = resolve_init(None).expect("resolve");

        assert_eq!(layout.vault_dir, expected);
        assert_eq!(layout.worktree, dir.path());
        std::env::set_current_dir(restore).expect("restore cwd");
    }

    #[test]
    fn resolve_init_rejects_existing_vault() {
        let dir = TempDir::new().expect("tempdir");
        let vault_dir = dir.path().join(VAULT_DIR);
        fs::create_dir_all(&vault_dir).expect("mkdir");
        fs::write(vault_dir.join(META_DB), b"").expect("touch meta.db");
        fs::write(vault_dir.join(CONFIG_FILE), b"x").expect("config");
        fs::create_dir_all(vault_dir.join(GIT_DIR)).expect("git");
        fs::write(vault_dir.join(README_FILE), b"x").expect("readme");

        let err = resolve_init(Some(vault_dir)).expect_err("should fail");
        assert!(matches!(err, VaultError::AlreadyInitialized { .. }));
    }

    #[test]
    fn state_dir_honors_env_override() {
        let dir = TempDir::new().expect("tempdir");
        let _guard = STATE_ENV_LOCK.lock().expect("lock");
        std::env::set_var(STATE_DIR_ENV, dir.path());
        assert_eq!(state_dir().expect("state dir"), dir.path());
        std::env::remove_var(STATE_DIR_ENV);
    }
}
