//! Singleton daemon advisory lock.

use std::fs::{File, OpenOptions};

use chrono::Utc;
use fs4::FileExt;

use crate::error::VaultError;
use crate::paths::daemon_lock_path;

use super::ensure_parent_dir;
use super::heartbeat::{read_heartbeat, write_heartbeat};

/// Guard holding the daemon advisory lock for the process lifetime.
pub struct DaemonGuard {
    _lock: File,
    started_at: String,
}

impl DaemonGuard {
    /// Try to acquire the singleton daemon lock.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::DaemonAlreadyRunning`] when another process holds the lock.
    pub fn acquire() -> Result<Self, VaultError> {
        let lock = open_daemon_lock_file()?;
        claim_exclusive_lock(&lock)?;
        let started_at = Utc::now().to_rfc3339();
        write_heartbeat(&started_at, 0)?;
        Ok(Self {
            _lock: lock,
            started_at,
        })
    }

    /// Update heartbeat with the current vault count.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the heartbeat cannot be written.
    pub fn heartbeat(&self, vault_count: usize) -> Result<(), VaultError> {
        write_heartbeat(&self.started_at, vault_count)
    }
}

fn open_daemon_lock_file() -> Result<File, VaultError> {
    let path = daemon_lock_path()?;
    ensure_parent_dir(&path)?;
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(Into::into)
}

fn claim_exclusive_lock(lock: &File) -> Result<(), VaultError> {
    if FileExt::try_lock(lock).is_ok() {
        return Ok(());
    }
    Err(lock_held_error())
}

fn lock_held_error() -> VaultError {
    if let Some(pid) = read_heartbeat().map(|h| h.pid) {
        return VaultError::DaemonAlreadyRunning { pid };
    }
    VaultError::LockHeld
}
