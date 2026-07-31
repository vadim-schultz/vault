//! `vault status` report assembly.

use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::daemon::{self, DaemonHeartbeat};
use crate::error::VaultError;
use crate::paths::VaultPaths;
use crate::registry::{VaultEntry, VaultRegistry};
use crate::service::{self, ServiceState};
use crate::storage::sqlite;

/// Overall daemon status for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatus {
    /// Whether the daemon lock/heartbeat indicate a running process.
    pub running: bool,
    /// Service manager state, if any.
    pub service_state: ServiceState,
    /// Latest heartbeat payload.
    pub heartbeat: Option<DaemonHeartbeat>,
    /// Seconds since the last heartbeat update.
    pub heartbeat_age_secs: Option<i64>,
}

/// Per-vault status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultStatus {
    /// Vault worktree root.
    pub root: PathBuf,
    /// Registration timestamp.
    pub registered_at: String,
    /// Last snapshot timestamp from `meta.db`.
    pub last_snapshot: Option<String>,
    /// Whether the vault root still exists.
    pub root_exists: bool,
}

/// Full status report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// Daemon subsection.
    pub daemon: DaemonStatus,
    /// Registered vaults.
    pub vaults: Vec<VaultStatus>,
}

/// Build the current status report.
///
/// # Errors
///
/// Returns [`VaultError`] when registry or sqlite queries fail.
pub fn report() -> Result<StatusReport, VaultError> {
    let registry = load_registry()?;
    let daemon = collect_daemon_status();
    let vaults = collect_vault_statuses(&registry)?;
    Ok(StatusReport { daemon, vaults })
}

impl StatusReport {
    /// Build a status report from global state.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when registry or sqlite queries fail.
    pub fn collect() -> Result<Self, VaultError> {
        report()
    }
}

impl fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.running {
            let pid = self
                .heartbeat
                .as_ref()
                .map_or_else(|| "unknown".to_string(), |h| h.pid.to_string());
            writeln!(f, "Daemon: running (pid {pid})")?;
        } else {
            writeln!(f, "Daemon: stopped")?;
        }
        if let Some(age) = self.heartbeat_age_secs {
            writeln!(f, "Heartbeat age: {age}s")?;
        }
        Ok(())
    }
}

impl fmt::Display for VaultStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.last_snapshot.as_deref().unwrap_or("never");
        let state = if self.root_exists { "ok" } else { "missing" };
        write!(
            f,
            "  {} [{state}] last snapshot: {snapshot}",
            self.root.display()
        )
    }
}

impl fmt::Display for StatusReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.daemon)?;
        write!(f, "Vaults: {}", self.vaults.len())?;
        for vault in &self.vaults {
            write!(f, "\n{vault}")?;
        }
        Ok(())
    }
}

fn load_registry() -> Result<VaultRegistry, VaultError> {
    let mut registry = VaultRegistry::load()?;
    registry.prune_stale()?;
    Ok(registry)
}

fn collect_daemon_status() -> DaemonStatus {
    let heartbeat = daemon::read_heartbeat();
    DaemonStatus {
        running: daemon::is_running(),
        service_state: service::for_current_platform().state(),
        heartbeat_age_secs: heartbeat.as_ref().and_then(heartbeat_age_secs),
        heartbeat,
    }
}

fn heartbeat_age_secs(beat: &DaemonHeartbeat) -> Option<i64> {
    DateTime::parse_from_rfc3339(&beat.updated_at)
        .ok()
        .map(|updated| {
            Utc::now()
                .signed_duration_since(updated.with_timezone(&Utc))
                .num_seconds()
        })
}

fn collect_vault_statuses(registry: &VaultRegistry) -> Result<Vec<VaultStatus>, VaultError> {
    registry.vault.iter().map(vault_status_for_entry).collect()
}

fn vault_status_for_entry(entry: &VaultEntry) -> Result<VaultStatus, VaultError> {
    let paths = vault_paths_for(&entry.root);
    Ok(VaultStatus {
        root: entry.root.clone(),
        registered_at: entry.registered_at.to_rfc3339(),
        last_snapshot: last_snapshot_for(&paths)?,
        root_exists: entry.root.is_dir(),
    })
}

fn vault_paths_for(root: &std::path::Path) -> VaultPaths {
    let vault_dir = root.join(crate::paths::VAULT_DIR);
    VaultPaths {
        worktree: root.to_path_buf(),
        vault_dir,
    }
}

fn last_snapshot_for(paths: &VaultPaths) -> Result<Option<String>, VaultError> {
    let meta_db = paths.meta_db_path();
    if meta_db.is_file() {
        sqlite::last_snapshot_time(&meta_db)
    } else {
        Ok(None)
    }
}
