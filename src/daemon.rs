//! Singleton daemon lock, heartbeat, and process lifecycle.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::process;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::adapters::{InMemoryQueueStore, TomlRegistry};
use crate::app::prune;
use crate::domain::{QueuedTask, TaskKind};
use crate::error::VaultError;
use crate::paths::{daemon_heartbeat_path, daemon_lock_path, daemon_log_path, daemon_queue_path};
use crate::queue::WorkQueue;
use crate::registry::VaultRegistry;
use crate::watcher;

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

/// Daemon work-queue snapshot written to `queue.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueSnapshot {
    /// When the snapshot was written.
    pub updated_at: String,
    /// Pending tasks in FIFO order.
    pub tasks: Vec<QueueTaskSnapshot>,
}

/// One pending task in a queue snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueTaskSnapshot {
    /// Task identifier.
    pub id: u64,
    /// Stable task kind name.
    pub kind: String,
    /// Scheduling lane.
    pub lane: String,
    /// Claim attempt count.
    pub attempts: u32,
}

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

fn lock_is_held() -> bool {
    let Ok(path) = daemon_lock_path() else {
        return false;
    };
    let Ok(lock) = OpenOptions::new().read(true).write(true).open(path) else {
        return false;
    };
    FileExt::try_lock(&lock).is_err()
}

/// Run the daemon in the foreground until interrupted or the watcher exits.
///
/// # Errors
///
/// Returns [`VaultError::DaemonAlreadyRunning`] when another daemon holds the lock.
pub async fn run_foreground() -> Result<(), VaultError> {
    let guard = Arc::new(DaemonGuard::acquire()?);
    append_log("vault daemon started")?;

    let store = Arc::new(InMemoryQueueStore::new());
    let queue = Arc::new(WorkQueue::new(store));
    seed_background_tasks(&queue)?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_task = spawn_shutdown_listener(shutdown_tx);
    let heartbeat_task = spawn_heartbeat_task(Arc::clone(&guard), Arc::clone(&queue));
    let runner_task = crate::queue::spawn_runner(Arc::clone(&queue));

    let watcher_result = watcher::run(shutdown_rx).await;

    shutdown_task.abort();
    heartbeat_task.abort();
    runner_task.abort();
    drop(guard);
    append_log("vault daemon stopped")?;
    watcher_result
}

fn seed_background_tasks(queue: &WorkQueue) -> Result<(), VaultError> {
    let registry = VaultRegistry::load()?;
    for entry in &registry.vault {
        if entry.enabled && entry.root.is_dir() {
            let _ = queue.enqueue(TaskKind::reconcile_walk(entry.root.clone()))?;
            let _ = queue.enqueue(TaskKind::git_housekeeping(entry.root.clone()))?;
        }
    }
    Ok(())
}

fn spawn_heartbeat_task(guard: Arc<DaemonGuard>, queue: Arc<WorkQueue>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let count = VaultRegistry::load().map_or(0, |r| r.vault.len());
            let _ = guard.heartbeat(count);
            let _ = write_queue_snapshot(&queue);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}

fn spawn_shutdown_listener(shutdown_tx: watch::Sender<bool>) -> JoinHandle<()> {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = shutdown_tx.send(true);
        }
    })
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

fn ensure_parent_dir(path: &std::path::Path) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
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

fn write_heartbeat(started_at: &str, vault_count: usize) -> Result<(), VaultError> {
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

fn write_queue_snapshot(queue: &WorkQueue) -> Result<(), VaultError> {
    let tasks = queue
        .snapshot()?
        .into_iter()
        .map(queue_task_snapshot)
        .collect();
    let snapshot = QueueSnapshot {
        updated_at: Utc::now().to_rfc3339(),
        tasks,
    };
    let path = daemon_queue_path()?;
    ensure_parent_dir(&path)?;
    let contents = serde_json::to_string_pretty(&snapshot)?;
    std::fs::write(path, contents)?;
    Ok(())
}

fn queue_task_snapshot(task: QueuedTask) -> QueueTaskSnapshot {
    QueueTaskSnapshot {
        id: task.id.as_u64(),
        kind: task.kind.name().to_string(),
        lane: task.lane,
        attempts: task.attempts,
    }
}

/// Read the current queue snapshot, if present.
#[must_use]
pub fn read_queue_snapshot() -> Option<QueueSnapshot> {
    let path = daemon_queue_path().ok()?;
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Append a line to `daemon.log`.
///
/// # Errors
///
/// Returns [`VaultError`] when the log file cannot be written.
pub fn append_log(message: &str) -> Result<(), VaultError> {
    let path = daemon_log_path()?;
    ensure_parent_dir(&path)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = format!("{} {message}\n", Utc::now().to_rfc3339());
    file.write_all(line.as_bytes())?;
    Ok(())
}

/// Spawn a detached daemon process when no service manager is available.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the child process cannot be spawned.
pub fn spawn_detached() -> Result<(), VaultError> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// Prune stale registry entries (daemon maintenance).
///
/// # Errors
///
/// Returns [`VaultError`] when registry load or save fails.
pub fn prune_registry() -> Result<usize, VaultError> {
    prune::prune(&TomlRegistry)
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

    #[test]
    fn queue_snapshot_roundtrip() {
        let _guard = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let dir = TempDir::new().expect("tempdir");
        std::env::set_var(crate::paths::STATE_DIR_ENV, dir.path());
        let store = std::sync::Arc::new(crate::adapters::InMemoryQueueStore::new());
        let queue = crate::queue::WorkQueue::new(store);
        let _ = queue.enqueue(crate::domain::TaskKind::reconcile_walk_once(
            std::path::PathBuf::from("/tmp/vault"),
        ));
        write_queue_snapshot(&queue).expect("write");
        let snap = read_queue_snapshot().expect("read");
        assert_eq!(snap.tasks.len(), 1);
        assert_eq!(snap.tasks[0].kind, "reconcile_walk");
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }
}
