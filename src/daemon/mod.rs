//! Singleton daemon lock, heartbeat, and process lifecycle.

mod guard;
mod heartbeat;
mod queue_snapshot;

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::adapters::{InMemoryQueueStore, TomlRegistry};
use crate::app::prune;
use crate::domain::TaskKind;
use crate::error::VaultError;
use crate::paths::daemon_log_path;
use crate::queue::WorkQueue;
use crate::registry::VaultRegistry;
use crate::watcher;

pub use guard::DaemonGuard;
pub use heartbeat::{read_heartbeat, DaemonHeartbeat, is_running};
pub use queue_snapshot::{read_queue_snapshot, QueueSnapshot, QueueTaskSnapshot};

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
            let _ = queue_snapshot::write_queue_snapshot(&queue);
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

fn ensure_parent_dir(path: &std::path::Path) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
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
