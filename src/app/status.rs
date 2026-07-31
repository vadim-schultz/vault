//! `vault status` use-case (read-only).

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::adapters::TomlRegistry;
use crate::daemon::{self, DaemonHeartbeat};
use crate::domain::VaultLayout;
use crate::error::VaultError;
use crate::ports::{RegistryStore, ServiceManager, ServiceState};
use crate::registry::{VaultEntry, VaultRegistry};
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

/// Build the current status report (read-only — does not prune registry).
///
/// # Errors
///
/// Returns [`VaultError`] when registry or vault metadata cannot be read.
pub fn report(
    registry: &dyn RegistryStore,
    service: &dyn ServiceManager,
) -> Result<StatusReport, VaultError> {
    let registry = registry.load()?;
    let daemon = collect_daemon_status(service);
    let vaults = collect_vault_statuses(&registry)?;
    Ok(StatusReport { daemon, vaults })
}

/// Build a status report using production adapters.
///
/// # Errors
///
/// Returns [`VaultError`] when registry or vault metadata cannot be read.
pub fn report_default() -> Result<StatusReport, VaultError> {
    let service: Box<dyn ServiceManager> = if crate::paths::skip_service() {
        Box::new(crate::adapters::NoopService)
    } else if crate::adapters::SystemdService::is_available() {
        Box::new(crate::adapters::SystemdService)
    } else {
        Box::new(crate::adapters::DetachedSpawnService)
    };
    report(&TomlRegistry, service.as_ref())
}

fn collect_daemon_status(service: &dyn ServiceManager) -> DaemonStatus {
    let heartbeat = daemon::read_heartbeat();
    DaemonStatus {
        running: daemon::is_running(),
        service_state: service.state(),
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
    let layout = VaultLayout::from_worktree(entry.root.clone());
    Ok(VaultStatus {
        root: entry.root.clone(),
        registered_at: entry.registered_at.to_rfc3339(),
        last_snapshot: last_snapshot_for(&layout)?,
        root_exists: entry.root.is_dir(),
    })
}

fn last_snapshot_for(layout: &VaultLayout) -> Result<Option<String>, VaultError> {
    let meta_db = layout.meta_db_path();
    if meta_db.is_file() {
        sqlite::last_snapshot_time(&meta_db)
    } else {
        Ok(None)
    }
}
