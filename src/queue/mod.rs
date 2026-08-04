//! Background work queue orchestration and runner.

pub mod handlers;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::domain::{QueuedTask, TaskId, TaskKind};
use crate::error::VaultError;
use crate::ports::queue::{FailOutcome, QueueStore, DEFAULT_LANE};

/// Idle poll when no work is available.
const IDLE_POLL_MS: u64 = 500;

/// Facade over a [`QueueStore`] with wake notifications for the runner.
pub struct WorkQueue {
    store: Arc<dyn QueueStore>,
    notify: Arc<Notify>,
}

impl WorkQueue {
    /// Create a work queue backed by `store`.
    #[must_use]
    pub fn new(store: Arc<dyn QueueStore>) -> Self {
        Self {
            store,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Enqueue `kind` on the default lane and wake the runner.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when scheduling fails.
    pub fn enqueue(&self, kind: TaskKind) -> Result<TaskId, VaultError> {
        let id = self.store.schedule(kind, DEFAULT_LANE)?;
        self.notify.notify_one();
        Ok(id)
    }

    /// Non-destructive peek at pending tasks on the default lane.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the store cannot be read.
    pub fn snapshot(&self) -> Result<Vec<QueuedTask>, VaultError> {
        self.store.snapshot(DEFAULT_LANE)
    }

    /// Shared store handle (for runner internals).
    #[must_use]
    pub fn store(&self) -> Arc<dyn QueueStore> {
        Arc::clone(&self.store)
    }

    fn notify(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }
}

/// Spawn the background runner loop.
#[must_use]
pub fn spawn_runner(queue: Arc<WorkQueue>) -> JoinHandle<()> {
    tokio::spawn(async move {
        runner_loop(queue).await;
    })
}

async fn runner_loop(queue: Arc<WorkQueue>) {
    let store = queue.store();
    let notify = queue.notify();
    loop {
        match store.claim_next_pending(DEFAULT_LANE) {
            Ok(Some(task)) => process_task(Arc::clone(&queue), &store, task).await,
            Ok(None) => wait_for_work(&notify).await,
            Err(err) => wait_after_claim_error(err, &notify).await,
        }
    }
}

async fn wait_after_claim_error(err: VaultError, notify: &Arc<Notify>) {
    let _ = crate::daemon::append_log(&format!("queue claim error: {err}"));
    wait_for_work(notify).await;
}

async fn wait_for_work(notify: &Arc<Notify>) {
    tokio::select! {
        () = notify.notified() => {}
        () = tokio::time::sleep(Duration::from_millis(IDLE_POLL_MS)) => {}
    }
}

async fn process_task(queue: Arc<WorkQueue>, store: &Arc<dyn QueueStore>, task: QueuedTask) {
    let id = task.id;
    let kind = task.kind.clone();
    let interval = kind.interval();
    let result = tokio::task::spawn_blocking(move || handlers::run(&kind)).await;

    let finished = match result {
        Ok(Ok(())) => match store.mark_complete(id) {
            Ok(()) => true,
            Err(err) => {
                let _ = crate::daemon::append_log(&format!("queue complete error: {err}"));
                false
            }
        },
        Ok(Err(err)) => match store.mark_failed(id, &err.to_string()) {
            Ok(FailOutcome::Dropped) => true,
            Ok(FailOutcome::Requeued) => false,
            Err(mark_err) => {
                let _ = crate::daemon::append_log(&format!("queue fail error: {mark_err}"));
                false
            }
        },
        Err(_) => match store.mark_failed(id, "background task panicked") {
            Ok(FailOutcome::Dropped) => true,
            Ok(FailOutcome::Requeued) => false,
            Err(mark_err) => {
                let _ = crate::daemon::append_log(&format!("queue fail error: {mark_err}"));
                false
            }
        },
    };

    if finished {
        maybe_reschedule(queue, &task.kind, interval);
    }
}

fn maybe_reschedule(queue: Arc<WorkQueue>, kind: &TaskKind, interval: Option<Duration>) {
    let Some(interval) = interval else {
        return;
    };
    let next_kind = kind.clone();
    tokio::spawn(async move {
        tokio::time::sleep(interval).await;
        let _ = queue.enqueue(next_kind);
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::adapters::InMemoryQueueStore;
    use crate::domain::TaskKind;

    #[test]
    fn enqueue_returns_task_id() {
        let store = Arc::new(InMemoryQueueStore::new());
        let queue = WorkQueue::new(store);
        let id = queue
            .enqueue(TaskKind::reconcile_walk_once(PathBuf::from("/vault")))
            .expect("enqueue");
        let snap = queue.snapshot().expect("snapshot");
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].id, id);
    }

    #[tokio::test]
    async fn enqueue_wakes_runner() {
        let store = Arc::new(InMemoryQueueStore::new()) as Arc<dyn QueueStore>;
        let queue = Arc::new(WorkQueue::new(Arc::clone(&store)));
        let runner = spawn_runner(Arc::clone(&queue));

        let vault = tempfile::TempDir::new().expect("tempdir");
        let layout = crate::domain::VaultLayout::from_worktree(vault.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        crate::storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git");
        crate::storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite");
        crate::config::VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("config");
        std::fs::write(layout.readme_path(), b"test").expect("readme");

        queue
            .enqueue(TaskKind::reconcile_walk_once(vault.path().to_path_buf()))
            .expect("enqueue");

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(queue.snapshot().expect("snapshot").is_empty());
        runner.abort();
    }

    #[tokio::test]
    async fn recurring_task_reschedules_after_interval() {
        let store = Arc::new(InMemoryQueueStore::new()) as Arc<dyn QueueStore>;
        let queue = Arc::new(WorkQueue::new(Arc::clone(&store)));
        let runner = spawn_runner(Arc::clone(&queue));

        let vault = tempfile::TempDir::new().expect("tempdir");
        let layout = crate::domain::VaultLayout::from_worktree(vault.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        crate::storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git");
        crate::storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite");
        crate::config::VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("config");
        std::fs::write(layout.readme_path(), b"test").expect("readme");

        queue
            .enqueue(TaskKind::reconcile_walk_with_interval(
                vault.path().to_path_buf(),
                Duration::from_millis(50),
            ))
            .expect("enqueue");

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(queue.snapshot().expect("snapshot").is_empty());
        runner.abort();

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(queue.snapshot().expect("snapshot").len(), 1);
    }

    #[tokio::test]
    async fn one_shot_task_does_not_reschedule() {
        let store = Arc::new(InMemoryQueueStore::new()) as Arc<dyn QueueStore>;
        let queue = Arc::new(WorkQueue::new(Arc::clone(&store)));
        let runner = spawn_runner(Arc::clone(&queue));

        let vault = tempfile::TempDir::new().expect("tempdir");
        let layout = crate::domain::VaultLayout::from_worktree(vault.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        crate::storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git");
        crate::storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite");
        crate::config::VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("config");
        std::fs::write(layout.readme_path(), b"test").expect("readme");

        queue
            .enqueue(TaskKind::reconcile_walk_once(vault.path().to_path_buf()))
            .expect("enqueue");

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(queue.snapshot().expect("snapshot").is_empty());

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(queue.snapshot().expect("snapshot").is_empty());

        runner.abort();
    }
}
