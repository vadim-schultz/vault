//! Daemon work-queue snapshot persistence.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::domain::QueuedTask;
use crate::error::VaultError;
use crate::paths::daemon_queue_path;
use crate::queue::WorkQueue;

use super::ensure_parent_dir;

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

/// Read the current queue snapshot, if present.
#[must_use]
pub fn read_queue_snapshot() -> Option<QueueSnapshot> {
    let path = daemon_queue_path().ok()?;
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub(crate) fn write_queue_snapshot(queue: &WorkQueue) -> Result<(), VaultError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
