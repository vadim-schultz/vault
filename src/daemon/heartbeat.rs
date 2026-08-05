//! Daemon heartbeat persistence.

use std::fs::OpenOptions;
use std::process;

use chrono::Utc;
use fs4::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::VaultError;
use crate::paths::{daemon_heartbeat_path, daemon_lock_path};

use super::ensure_parent_dir;

/// Daemon heartbeat written to `daemon.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonHeartbeat {
    /// Process id of the daemon.
    pub pid: u32,
    /// When the daemon started.
    pub started_at: String,
    /// When the heartbeat was last updated.
    pub updated_at: String,
    /// Number of registered vaults.
    pub vault_count: usize,
    /// Vault binary version.
    pub version: String,
}

/// Read the current daemon heartbeat, if present.
#[must_use]
pub fn read_heartbeat() -> Option<DaemonHeartbeat> {
    let path = daemon_heartbeat_path().ok()?;
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Return whether the daemon appears to be running from its heartbeat and lock.
#[must_use]
pub fn is_running() -> bool {
    read_heartbeat().is_some() && lock_is_held()
}

pub(crate) fn write_heartbeat(started_at: &str, vault_count: usize) -> Result<(), VaultError> {
    let heartbeat = DaemonHeartbeat {
        pid: process::id(),
        started_at: started_at.to_string(),
        updated_at: Utc::now().to_rfc3339(),
        vault_count,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let path = daemon_heartbeat_path()?;
    ensure_parent_dir(&path)?;
    let contents = serde_json::to_string_pretty(&heartbeat)?;
    std::fs::write(path, contents)?;
    Ok(())
}

fn lock_is_held() -> bool {
    let Ok(path) = daemon_lock_path() else {
        return false;
    };
    let Ok(lock) = OpenOptions::new().read(true).write(true).open(path) else {
        return false;
    };
    FileExt::try_lock(&lock).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn heartbeat_roundtrip() {
        let _guard = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let dir = TempDir::new().expect("tempdir");
        std::env::set_var(crate::paths::STATE_DIR_ENV, dir.path());
        write_heartbeat("start", 2).expect("write");
        let beat = read_heartbeat().expect("read");
        assert_eq!(beat.vault_count, 2);
        assert_eq!(beat.version, env!("CARGO_PKG_VERSION"));
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }
}
