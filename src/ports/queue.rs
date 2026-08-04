#![allow(clippy::missing_errors_doc)]

//! Background work queue storage port.

use crate::domain::{QueuedTask, TaskId, TaskKind};
use crate::error::VaultError;

/// Default scheduling lane for MVP FIFO queue.
pub const DEFAULT_LANE: &str = "default";

/// Whether a failed task was requeued or dropped after retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOutcome {
    /// Task was requeued for another attempt.
    Requeued,
    /// Task was dropped after exhausting retries.
    Dropped,
}

/// Storage/ordering backend for queued background tasks — the swappable mechanism.
pub trait QueueStore: Send + Sync {
    /// Enqueue `kind` on `lane` and return its id.
    fn schedule(&self, kind: TaskKind, lane: &str) -> Result<TaskId, VaultError>;

    /// Atomically claim the next pending task on `lane`, if any.
    fn claim_next_pending(&self, lane: &str) -> Result<Option<QueuedTask>, VaultError>;

    /// Remove a completed task from the store.
    fn mark_complete(&self, id: TaskId) -> Result<(), VaultError>;

    /// Record failure; requeue or drop depending on attempt count.
    fn mark_failed(&self, id: TaskId, error: &str) -> Result<FailOutcome, VaultError>;

    /// Non-destructive peek at pending tasks on `lane`.
    fn snapshot(&self, lane: &str) -> Result<Vec<QueuedTask>, VaultError>;
}

#[cfg(test)]
pub mod contract {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::domain::TaskKind;

    /// FIFO order within a lane.
    pub fn schedule_claims_fifo(store: Arc<dyn QueueStore>) {
        let first = store
            .schedule(
                TaskKind::reconcile_walk_once(PathBuf::from("/a")),
                DEFAULT_LANE,
            )
            .expect("schedule first");
        let second = store
            .schedule(
                TaskKind::reconcile_walk_once(PathBuf::from("/b")),
                DEFAULT_LANE,
            )
            .expect("schedule second");

        let claimed_first = store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .expect("some");
        assert_eq!(claimed_first.id, first);

        let claimed_second = store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .expect("some");
        assert_eq!(claimed_second.id, second);
    }

    /// Empty lane returns `None`.
    pub fn empty_lane_returns_none(store: Arc<dyn QueueStore>) {
        assert!(store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .is_none());
    }

    /// Completed tasks are removed.
    pub fn mark_complete_removes_task(store: Arc<dyn QueueStore>) {
        let id = store
            .schedule(
                TaskKind::reconcile_walk_once(PathBuf::from("/a")),
                DEFAULT_LANE,
            )
            .expect("schedule");
        store.mark_complete(id).expect("complete");
        assert!(store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .is_none());
    }

    /// Failed tasks retry up to cap then drop.
    pub fn mark_failed_retries_then_drops(store: Arc<dyn QueueStore>) {
        let id = store
            .schedule(
                TaskKind::reconcile_walk_once(PathBuf::from("/a")),
                DEFAULT_LANE,
            )
            .expect("schedule");
        let _ = store.claim_next_pending(DEFAULT_LANE).expect("claim");

        assert_eq!(
            store.mark_failed(id, "err").expect("fail"),
            FailOutcome::Requeued
        );
        let requeued = store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .expect("requeued");
        assert_eq!(requeued.attempts, 2);

        assert_eq!(
            store.mark_failed(id, "err").expect("fail"),
            FailOutcome::Requeued
        );
        let _ = store.claim_next_pending(DEFAULT_LANE).expect("claim");

        assert_eq!(
            store.mark_failed(id, "err").expect("fail"),
            FailOutcome::Dropped
        );
        assert!(store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .is_none());
    }

    /// Snapshot returns pending tasks without claiming them.
    pub fn snapshot_is_non_destructive(store: Arc<dyn QueueStore>) {
        assert!(store.snapshot(DEFAULT_LANE).expect("snapshot").is_empty());

        store
            .schedule(
                TaskKind::reconcile_walk_once(PathBuf::from("/a")),
                DEFAULT_LANE,
            )
            .expect("schedule");
        store
            .schedule(
                TaskKind::reconcile_walk_once(PathBuf::from("/b")),
                DEFAULT_LANE,
            )
            .expect("schedule");

        let snap = store.snapshot(DEFAULT_LANE).expect("snapshot");
        assert_eq!(snap.len(), 2);

        let claimed = store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .expect("some");
        assert_eq!(claimed.id, snap[0].id);
    }

    /// Custom interval is preserved through schedule/claim.
    pub fn interval_preserved_on_round_trip(store: Arc<dyn QueueStore>) {
        let kind = TaskKind::reconcile_walk_with_interval(
            PathBuf::from("/vault"),
            Duration::from_millis(50),
        );
        store.schedule(kind, DEFAULT_LANE).expect("schedule");
        let claimed = store
            .claim_next_pending(DEFAULT_LANE)
            .expect("claim")
            .expect("some");
        assert_eq!(claimed.kind.interval(), Some(Duration::from_millis(50)));
    }
}
