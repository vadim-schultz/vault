//! In-memory FIFO queue store for the daemon work queue.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::domain::{QueuedTask, TaskId, TaskKind};
use crate::error::VaultError;
use crate::ports::queue::{FailOutcome, QueueStore};

/// Maximum claim attempts before a failed task is dropped.
const MAX_ATTEMPTS: u32 = 3;

/// FIFO pending queues per lane with atomic claim semantics.
pub struct InMemoryQueueStore {
    next_id: AtomicU64,
    inner: Mutex<StoreInner>,
}

struct StoreInner {
    pending_ids: HashMap<String, VecDeque<TaskId>>,
    tasks: HashMap<TaskId, QueuedTask>,
}

impl StoreInner {
    fn pending_queue_mut(&mut self, lane: &str) -> Option<&mut VecDeque<TaskId>> {
        self.pending_ids.get_mut(lane)
    }

    fn pop_pending_id(queue: &mut VecDeque<TaskId>) -> Option<TaskId> {
        queue.pop_front()
    }

    fn record_claim(&mut self, id: TaskId) -> Option<QueuedTask> {
        let task = self.tasks.get_mut(&id)?;
        task.attempts += 1;
        Some(task.clone())
    }

    fn require_task(&self, id: TaskId) -> Result<&QueuedTask, VaultError> {
        self.tasks
            .get(&id)
            .ok_or(VaultError::TaskNotFound { id: id.as_u64() })
    }

    fn drop_if_attempts_exhausted(&mut self, id: TaskId, attempts: u32) -> Option<FailOutcome> {
        if attempts < MAX_ATTEMPTS {
            return None;
        }
        self.tasks.remove(&id);
        Some(FailOutcome::Dropped)
    }

    fn requeue_task(&mut self, id: TaskId, lane: &str) {
        self.pending_ids
            .entry(lane.to_string())
            .or_default()
            .push_back(id);
    }
}

impl InMemoryQueueStore {
    /// Create an empty in-memory queue store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            inner: Mutex::new(StoreInner {
                pending_ids: HashMap::new(),
                tasks: HashMap::new(),
            }),
        }
    }
}

impl Default for InMemoryQueueStore {
    fn default() -> Self {
        Self::new()
    }
}

impl QueueStore for InMemoryQueueStore {
    fn schedule(&self, kind: TaskKind, lane: &str) -> Result<TaskId, VaultError> {
        let id = TaskId::new(self.next_id.fetch_add(1, Ordering::Relaxed));
        let task = QueuedTask {
            id,
            kind,
            lane: lane.to_string(),
            attempts: 0,
        };
        let mut inner = self.inner.lock().map_err(|_| VaultError::LockHeld)?;
        inner.tasks.insert(id, task);
        inner
            .pending_ids
            .entry(lane.to_string())
            .or_default()
            .push_back(id);
        Ok(id)
    }

    fn claim_next_pending(&self, lane: &str) -> Result<Option<QueuedTask>, VaultError> {
        let mut inner = self.inner.lock().map_err(|_| VaultError::LockHeld)?;
        loop {
            let Some(queue) = inner.pending_queue_mut(lane) else {
                return Ok(None);
            };
            let Some(id) = StoreInner::pop_pending_id(queue) else {
                return Ok(None);
            };
            if let Some(task) = inner.record_claim(id) {
                return Ok(Some(task));
            }
        }
    }

    fn mark_complete(&self, id: TaskId) -> Result<(), VaultError> {
        let mut inner = self.inner.lock().map_err(|_| VaultError::LockHeld)?;
        inner.require_task(id)?;
        inner.tasks.remove(&id);
        Ok(())
    }

    fn mark_failed(&self, id: TaskId, _error: &str) -> Result<FailOutcome, VaultError> {
        let mut inner = self.inner.lock().map_err(|_| VaultError::LockHeld)?;
        let (attempts, lane) = {
            let task = inner.require_task(id)?;
            (task.attempts, task.lane.clone())
        };
        if let Some(outcome) = inner.drop_if_attempts_exhausted(id, attempts) {
            return Ok(outcome);
        }
        inner.requeue_task(id, &lane);
        Ok(FailOutcome::Requeued)
    }

    fn snapshot(&self, lane: &str) -> Result<Vec<QueuedTask>, VaultError> {
        let inner = self.inner.lock().map_err(|_| VaultError::LockHeld)?;
        let Some(queue) = inner.pending_ids.get(lane) else {
            return Ok(Vec::new());
        };
        let tasks = queue
            .iter()
            .filter_map(|id| inner.tasks.get(id).cloned())
            .collect();
        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::ports::queue::contract;

    fn store() -> Arc<dyn QueueStore> {
        Arc::new(InMemoryQueueStore::new())
    }

    #[test]
    fn contract_fifo() {
        contract::schedule_claims_fifo(store());
    }

    #[test]
    fn contract_empty_lane() {
        contract::empty_lane_returns_none(store());
    }

    #[test]
    fn contract_mark_complete() {
        contract::mark_complete_removes_task(store());
    }

    #[test]
    fn contract_mark_failed() {
        contract::mark_failed_retries_then_drops(store());
    }

    #[test]
    fn contract_snapshot() {
        contract::snapshot_is_non_destructive(store());
    }

    #[test]
    fn contract_interval_preserved() {
        contract::interval_preserved_on_round_trip(store());
    }
}
