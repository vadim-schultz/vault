//! `vault status` use-case (read-only).

mod model;

use chrono::{DateTime, Utc};

use crate::adapters::TomlRegistry;
use crate::daemon::{self, DaemonHeartbeat, QueueSnapshot};
use crate::domain::VaultLayout;
use crate::error::VaultError;
use crate::ports::{RegistryStore, ServiceManager};
use crate::registry::{VaultEntry, VaultRegistry};
use crate::storage::housekeeping;

pub use model::{
    DaemonStatus, QueueStatus, QueueTaskStatus, StatusReport, VaultHousekeepingStatus,
    VaultStatus,
};

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
    let queue = collect_queue_status();
    let vaults = collect_vault_statuses(&registry)?;
    Ok(StatusReport {
        daemon,
        queue,
        vaults,
    })
}

/// Build a status report using production adapters.
///
/// # Errors
///
/// Returns [`VaultError`] when registry or vault metadata cannot be read.
pub fn report_default() -> Result<StatusReport, VaultError> {
    let service: Box<dyn crate::ports::ServiceManager> = if crate::paths::skip_service() {
        Box::new(crate::adapters::NoopService)
    } else if crate::adapters::SystemdService::is_available() {
        Box::new(crate::adapters::SystemdService)
    } else {
        Box::new(crate::adapters::DetachedSpawnService)
    };
    report(&TomlRegistry, service.as_ref())
}

fn collect_daemon_status(service: &dyn crate::ports::ServiceManager) -> DaemonStatus {
    let heartbeat = daemon::read_heartbeat();
    DaemonStatus {
        running: daemon::is_running(),
        service_state: service.state(),
        heartbeat_age_secs: heartbeat.as_ref().and_then(heartbeat_age_secs),
        heartbeat,
    }
}

fn collect_queue_status() -> Option<QueueStatus> {
    if !daemon::is_running() {
        return None;
    }
    daemon::read_queue_snapshot().map(queue_status_from_snapshot)
}

fn queue_status_from_snapshot(snapshot: QueueSnapshot) -> QueueStatus {
    QueueStatus {
        updated_at: snapshot.updated_at,
        tasks: snapshot
            .tasks
            .into_iter()
            .map(|task| QueueTaskStatus {
                id: task.id,
                kind: task.kind,
                lane: task.lane,
                attempts: task.attempts,
            })
            .collect(),
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
        housekeeping: housekeeping_status_for(entry, &layout)?,
    })
}

fn housekeeping_status_for(
    entry: &VaultEntry,
    layout: &VaultLayout,
) -> Result<Option<VaultHousekeepingStatus>, VaultError> {
    if !entry.root.is_dir() || !layout.meta_db_path().is_file() {
        return Ok(None);
    }
    let status = housekeeping::status_for(layout)?;
    Ok(Some(VaultHousekeepingStatus {
        counts: status.counts,
        last_repack: status.last_repack,
    }))
}

fn last_snapshot_for(layout: &VaultLayout) -> Result<Option<String>, VaultError> {
    let meta_db = layout.meta_db_path();
    if meta_db.is_file() {
        crate::storage::sqlite::last_snapshot_time(&meta_db)
    } else {
        Ok(None)
    }
}
