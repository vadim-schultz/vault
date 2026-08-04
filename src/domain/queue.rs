//! Background task types for the daemon work queue.

use std::path::PathBuf;
use std::time::Duration;

/// Opaque identifier for a queued task, unique for the daemon process's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl TaskId {
    /// Create a task id from its numeric value.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Return the raw numeric identifier.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Kinds of background work the queue can carry, each owning its own recurrence.
///
/// Construct variants only through their `TaskKind::xxx(..)` constructor so the
/// recurrence interval lives with the kind's own definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskKind {
    /// Walk a vault's watch roots and diff against tracked files, logging any mismatch.
    ReconcileWalk {
        /// Vault worktree root.
        vault_root: PathBuf,
        /// Re-enqueue this often after this run finishes; `None` = run once.
        interval: Option<Duration>,
    },
}

impl TaskKind {
    const RECONCILE_WALK_INTERVAL: Duration = Duration::from_secs(600);

    /// Build a recurring reconciliation-walk task for `vault_root`.
    #[must_use]
    pub fn reconcile_walk(vault_root: PathBuf) -> Self {
        Self::ReconcileWalk {
            vault_root,
            interval: Some(Self::RECONCILE_WALK_INTERVAL),
        }
    }

    /// Build a one-shot reconciliation-walk task (for tests).
    #[must_use]
    pub fn reconcile_walk_once(vault_root: PathBuf) -> Self {
        Self::ReconcileWalk {
            vault_root,
            interval: None,
        }
    }

    /// Build a reconciliation-walk task with a custom interval (for tests).
    #[must_use]
    pub fn reconcile_walk_with_interval(vault_root: PathBuf, interval: Duration) -> Self {
        Self::ReconcileWalk {
            vault_root,
            interval: Some(interval),
        }
    }

    /// This task's own recurrence, if any — read by the runner after each run.
    #[must_use]
    pub fn interval(&self) -> Option<Duration> {
        match self {
            Self::ReconcileWalk { interval, .. } => *interval,
        }
    }

    /// Stable low-level name for logging and `vault status`.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ReconcileWalk { .. } => "reconcile_walk",
        }
    }

    /// Vault worktree root when this kind is vault-scoped.
    #[must_use]
    pub fn vault_root(&self) -> Option<&PathBuf> {
        match self {
            Self::ReconcileWalk { vault_root, .. } => Some(vault_root),
        }
    }
}

/// A task as held by a `QueueStore`, with its lane and retry bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedTask {
    /// Unique task identifier.
    pub id: TaskId,
    /// Work to perform.
    pub kind: TaskKind,
    /// Scheduling lane (FIFO within lane for MVP).
    pub lane: String,
    /// How many times this task has been claimed.
    pub attempts: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_walk_has_default_interval() {
        let kind = TaskKind::reconcile_walk(PathBuf::from("/tmp/vault"));
        assert_eq!(kind.interval(), Some(Duration::from_secs(600)));
        assert_eq!(kind.name(), "reconcile_walk");
    }

    #[test]
    fn reconcile_walk_once_has_no_interval() {
        let kind = TaskKind::reconcile_walk_once(PathBuf::from("/tmp/vault"));
        assert_eq!(kind.interval(), None);
    }
}
