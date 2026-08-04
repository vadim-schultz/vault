//! Status report DTOs for `vault status`.

use std::path::PathBuf;

use crate::daemon::DaemonHeartbeat;
use crate::ports::ServiceState;
use crate::storage::housekeeping::{self, RepackRecord};

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

/// Per-vault git housekeeping status for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultHousekeepingStatus {
    /// Live loose/pack counts from the object store.
    pub counts: housekeeping::ObjectCounts,
    /// Last repack record from `.vault/housekeeping.json`, if any.
    pub last_repack: Option<RepackRecord>,
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
    /// Git housekeeping counts and last-repack history.
    pub housekeeping: Option<VaultHousekeepingStatus>,
}

/// Background work-queue status from `queue.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueStatus {
    /// When the snapshot was written.
    pub updated_at: String,
    /// Pending tasks in FIFO order.
    pub tasks: Vec<QueueTaskStatus>,
}

/// One pending task in the queue snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueTaskStatus {
    /// Task identifier.
    pub id: u64,
    /// Stable task kind name.
    pub kind: String,
    /// Scheduling lane.
    pub lane: String,
    /// Claim attempt count.
    pub attempts: u32,
}

/// Full status report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// Daemon subsection.
    pub daemon: DaemonStatus,
    /// Background work queue, when the daemon has written a snapshot.
    pub queue: Option<QueueStatus>,
    /// Registered vaults.
    pub vaults: Vec<VaultStatus>,
}
