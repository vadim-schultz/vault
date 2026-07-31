//! Path resolution for vault initialization and discovery.

use std::path::{Path, PathBuf};

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

/// Resolved paths for `vault init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPaths {
    /// Vault root (parent of `.vault/`).
    pub worktree: PathBuf,
    /// `.vault/` directory path.
    pub vault_dir: PathBuf,
}

impl InitPaths {
    /// Path to `.vault/config.toml`.
    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.vault_dir.join(CONFIG_FILE)
    }

    /// Path to `.vault/meta.db`.
    #[must_use]
    pub fn meta_db_path(&self) -> PathBuf {
        self.vault_dir.join(META_DB)
    }

    /// Path to `.vault/.git/`.
    #[must_use]
    pub fn git_dir_path(&self) -> PathBuf {
        self.vault_dir.join(GIT_DIR)
    }

    /// Path to `.vault/README`.
    #[must_use]
    pub fn readme_path(&self) -> PathBuf {
        self.vault_dir.join(README_FILE)
    }
}

/// Resolved paths for an existing vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    /// Vault root (parent of `.vault/`).
    pub worktree: PathBuf,
    /// `.vault/` directory path.
    pub vault_dir: PathBuf,
}

impl VaultPaths {
    /// Path to `.vault/config.toml`.
    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.vault_dir.join(CONFIG_FILE)
    }

    /// Path to `.vault/meta.db`.
    #[must_use]
    pub fn meta_db_path(&self) -> PathBuf {
        self.vault_dir.join(META_DB)
    }

    /// Path to `.vault/.git/`.
    #[must_use]
    pub fn git_dir_path(&self) -> PathBuf {
        self.vault_dir.join(GIT_DIR)
    }
}

impl From<InitPaths> for VaultPaths {
    fn from(paths: InitPaths) -> Self {
        Self {
            worktree: paths.worktree,
            vault_dir: paths.vault_dir,
        }
    }
}

/// Return whether a vault directory already contains init artifacts.
#[must_use]
pub fn is_initialized(vault_dir: &Path) -> bool {
    [CONFIG_FILE, META_DB, GIT_DIR, README_FILE]
        .iter()
        .any(|name| vault_dir.join(name).exists())
}

/// Return the global vault state directory path.
///
/// The path may not exist yet; call [`ensure_state_dir`] before writing into it.
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
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the data directory cannot be resolved.
fn default_state_dir() -> Result<PathBuf, VaultError> {
    directories::ProjectDirs::from("", "", "vault")
        .ok_or_else(|| {
            VaultError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not resolve user data directory",
            ))
        })
        .map(|dirs| dirs.data_dir().to_path_buf())
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

/// Serialize unit tests that override [`STATE_DIR_ENV`].
#[cfg(test)]
pub static STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[must_use]
pub fn skip_service() -> bool {
    std::env::var(NO_SERVICE_ENV)
        .is_ok_and(|v| !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"))
}

/// Resolve worktree and `.vault/` paths for initialization.
///
/// # Errors
///
/// Returns [`VaultError::InvalidVaultPath`] when `--vault-path` has no parent,
/// or [`VaultError::AlreadyInitialized`] when init markers are present.
pub fn resolve_init(vault_path: Option<PathBuf>) -> Result<InitPaths, VaultError> {
    let paths = resolve_vault_paths(vault_path)?;
    if is_initialized(&paths.vault_dir) {
        return Err(VaultError::AlreadyInitialized {
            path: paths.vault_dir,
        });
    }
    Ok(paths)
}

/// Resolve paths for an existing vault.
///
/// # Errors
///
/// Returns [`VaultError::VaultNotFound`] when no `.vault/` exists.
pub fn resolve_vault(vault_path: Option<PathBuf>) -> Result<VaultPaths, VaultError> {
    let paths = resolve_vault_paths(vault_path)?;
    if !is_initialized(&paths.vault_dir) {
        return Err(VaultError::VaultNotFound {
            start: paths.worktree,
        });
    }
    Ok(VaultPaths {
        worktree: paths.worktree,
        vault_dir: paths.vault_dir,
    })
}

fn resolve_vault_paths(vault_path: Option<PathBuf>) -> Result<InitPaths, VaultError> {
    match vault_path {
        None => {
            let worktree = std::env::current_dir()?;
            let vault_dir = worktree.join(VAULT_DIR);
            Ok(InitPaths {
                worktree,
                vault_dir,
            })
        }
        Some(vault_path) => {
            let vault_dir = vault_path;
            let worktree = vault_dir
                .parent()
                .ok_or_else(|| VaultError::InvalidVaultPath {
                    path: vault_dir.clone(),
                })?;
            Ok(InitPaths {
                worktree: worktree.to_path_buf(),
                vault_dir,
            })
        }
    }
}

/// Discover `.vault/` by walking up from `start`.
#[must_use]
pub fn discover_vault(start: &Path) -> Option<PathBuf> {
    let mut current = start.canonicalize().ok()?;
    loop {
        let vault_dir = current.join(VAULT_DIR);
        if vault_dir.join(CONFIG_FILE).is_file() {
            return Some(vault_dir);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn resolve_init_defaults_to_cwd_vault() {
        let dir = TempDir::new().expect("tempdir");
        let expected = dir.path().join(VAULT_DIR);

        std::env::set_current_dir(dir.path()).expect("chdir");
        let paths = resolve_init(None).expect("resolve");

        assert_eq!(paths.vault_dir, expected);
        assert_eq!(paths.worktree, dir.path());
    }

    #[test]
    fn resolve_init_rejects_existing_vault() {
        let dir = TempDir::new().expect("tempdir");
        let vault_dir = dir.path().join(VAULT_DIR);
        fs::create_dir_all(&vault_dir).expect("mkdir");
        fs::write(vault_dir.join(META_DB), b"").expect("touch meta.db");

        let err = resolve_init(Some(vault_dir)).expect_err("should fail");
        assert!(matches!(err, VaultError::AlreadyInitialized { .. }));
    }

    #[test]
    fn is_initialized_detects_readme() {
        let dir = TempDir::new().expect("tempdir");
        let vault_dir = dir.path().join(VAULT_DIR);
        fs::create_dir_all(&vault_dir).expect("mkdir");
        fs::write(vault_dir.join(README_FILE), b"partial").expect("touch README");

        assert!(is_initialized(&vault_dir));
    }

    #[test]
    fn state_dir_honors_env_override() {
        let dir = TempDir::new().expect("tempdir");
        let _guard = STATE_ENV_LOCK.lock().expect("lock");
        std::env::set_var(STATE_DIR_ENV, dir.path());
        assert_eq!(state_dir().expect("state dir"), dir.path());
        std::env::remove_var(STATE_DIR_ENV);
    }

    #[test]
    fn discover_vault_finds_parent() {
        let dir = TempDir::new().expect("tempdir");
        let vault_dir = dir.path().join(VAULT_DIR);
        fs::create_dir_all(&vault_dir).expect("mkdir");
        fs::write(vault_dir.join(CONFIG_FILE), "watch_roots = [\".\"]\n").expect("config");

        let nested = dir.path().join("docs").join("deep");
        fs::create_dir_all(&nested).expect("mkdir nested");

        let found = discover_vault(&nested).expect("discover");
        assert_eq!(found, vault_dir.canonicalize().expect("canon"));
    }
}
