//! Test fakes for ports.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use crate::domain::{CommitSha, FileChange, RelPath, SnapshotRecord};
use crate::error::VaultError;
use crate::ports::{Clock, MetaIndex, ObjectStore, RegistryStore, ServiceManager, ServiceState};
use crate::registry::VaultRegistry;

/// Fixed clock for deterministic tests.
pub struct FixedClock {
    instant: DateTime<Utc>,
}

impl FixedClock {
    /// Create a clock fixed at `instant`.
    #[must_use]
    pub fn at(instant: DateTime<Utc>) -> Self {
        Self { instant }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.instant
    }
}

/// In-memory metadata index.
pub struct InMemoryMetaIndex {
    records: Mutex<Vec<SnapshotRecord>>,
}

impl Default for InMemoryMetaIndex {
    fn default() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
        }
    }
}

impl MetaIndex for InMemoryMetaIndex {
    fn record_snapshot(&self, record: &SnapshotRecord) -> Result<(), VaultError> {
        self.records
            .lock()
            .map_err(|_| VaultError::TaskPanicked)?
            .push(record.clone());
        Ok(())
    }

    fn last_snapshot_time(&self) -> Result<Option<String>, VaultError> {
        Ok(self
            .records
            .lock()
            .map_err(|_| VaultError::TaskPanicked)?
            .last()
            .map(|r| r.created_at.clone()))
    }

    fn resolve_at(&self, at: &str) -> Result<Option<CommitSha>, VaultError> {
        let records = self.records.lock().map_err(|_| VaultError::TaskPanicked)?;
        Ok(records
            .iter()
            .filter(|r| r.created_at.as_str() <= at)
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
            .map(|r| r.commit_sha.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::meta_index::contract;

    #[test]
    fn resolve_at_contract() {
        let index = Arc::new(InMemoryMetaIndex::default());
        contract::resolve_at_returns_latest_commit_at_or_before(index);
    }
}

/// In-memory object store.
pub struct InMemoryObjectStore {
    commits: Mutex<Vec<(CommitSha, Vec<FileChange>)>>,
}

impl Default for InMemoryObjectStore {
    fn default() -> Self {
        Self {
            commits: Mutex::new(Vec::new()),
        }
    }
}

impl ObjectStore for InMemoryObjectStore {
    fn commit(
        &self,
        changes: &[FileChange],
        _message: &str,
    ) -> Result<Option<CommitSha>, VaultError> {
        let sha = CommitSha(format!(
            "fake-{}",
            self.commits
                .lock()
                .map_err(|_| VaultError::TaskPanicked)?
                .len()
        ));
        self.commits
            .lock()
            .map_err(|_| VaultError::TaskPanicked)?
            .push((sha.clone(), changes.to_vec()));
        Ok(Some(sha))
    }

    fn read_blob(
        &self,
        _commit: &CommitSha,
        _path: &RelPath,
    ) -> Result<Option<Vec<u8>>, VaultError> {
        Ok(None)
    }
}

/// Records service manager start calls.
pub struct RecordingServiceManager {
    pub starts: Mutex<usize>,
}

impl Default for RecordingServiceManager {
    fn default() -> Self {
        Self {
            starts: Mutex::new(0),
        }
    }
}

impl ServiceManager for RecordingServiceManager {
    fn start(&self) -> Result<(), VaultError> {
        *self.starts.lock().map_err(|_| VaultError::TaskPanicked)? += 1;
        Ok(())
    }

    fn state(&self) -> ServiceState {
        ServiceState::Stopped
    }
}

/// In-memory registry store.
pub struct InMemoryRegistry {
    registry: Mutex<VaultRegistry>,
}

impl Default for InMemoryRegistry {
    fn default() -> Self {
        Self {
            registry: Mutex::new(VaultRegistry::default()),
        }
    }
}

impl RegistryStore for InMemoryRegistry {
    fn load(&self) -> Result<VaultRegistry, VaultError> {
        Ok(self
            .registry
            .lock()
            .map_err(|_| VaultError::TaskPanicked)?
            .clone())
    }

    fn save(&self, registry: &VaultRegistry) -> Result<(), VaultError> {
        *self.registry.lock().map_err(|_| VaultError::TaskPanicked)? = registry.clone();
        Ok(())
    }

    fn register(&self, root: &Path) -> Result<bool, VaultError> {
        let mut registry = self.registry.lock().map_err(|_| VaultError::TaskPanicked)?;
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if registry.vault.iter().any(|entry| entry.root == root) {
            return Ok(false);
        }
        registry.vault.push(crate::registry::VaultEntry {
            root,
            registered_at: chrono::Utc::now(),
            enabled: true,
        });
        Ok(true)
    }

    fn prune_stale(&self) -> Result<usize, VaultError> {
        let mut registry = self.registry.lock().map_err(|_| VaultError::TaskPanicked)?;
        let before = registry.vault.len();
        registry.vault.retain(|entry| entry.root.is_dir());
        Ok(before.saturating_sub(registry.vault.len()))
    }
}
