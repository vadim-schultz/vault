//! In-memory metadata index fake.

use std::sync::Mutex;

use crate::domain::{CommitSha, RelPath, SnapshotEntry, SnapshotRecord, TrackedFile};
use crate::error::VaultError;
use crate::ports::MetaIndex;

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

    fn list_snapshots(&self, path: Option<&RelPath>) -> Result<Vec<SnapshotEntry>, VaultError> {
        let records = self.records.lock().map_err(|_| VaultError::TaskPanicked)?;
        let mut entries: Vec<SnapshotEntry> = records
            .iter()
            .rev()
            .filter_map(|record| match path {
                None => Some(SnapshotEntry {
                    commit_sha: record.commit_sha.clone(),
                    created_at: record.created_at.clone(),
                    event: None,
                }),
                Some(p) => {
                    record
                        .changes
                        .iter()
                        .find(|c| c.rel == *p)
                        .map(|change| SnapshotEntry {
                            commit_sha: record.commit_sha.clone(),
                            created_at: record.created_at.clone(),
                            event: Some(change.kind),
                        })
                }
            })
            .collect();
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(entries)
    }

    fn list_tracked_files(&self) -> Result<Vec<TrackedFile>, VaultError> {
        use std::collections::HashMap;

        let records = self.records.lock().map_err(|_| VaultError::TaskPanicked)?;
        let mut latest: HashMap<String, (String, crate::domain::FileEventKind)> = HashMap::new();

        for record in records.iter() {
            for change in &record.changes {
                latest.insert(
                    change.rel.as_str().to_string(),
                    (record.created_at.clone(), change.kind),
                );
            }
        }

        let mut tracked: Vec<TrackedFile> = latest
            .into_iter()
            .filter(|(_, (_, kind))| *kind != crate::domain::FileEventKind::Delete)
            .map(|(path, (last_modified, _))| TrackedFile {
                path: RelPath::parse(&path),
                last_modified,
            })
            .collect();
        tracked.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
        Ok(tracked)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::ports::meta_index::contract;

    #[test]
    fn resolve_at_contract() {
        let index = Arc::new(InMemoryMetaIndex::default());
        contract::resolve_at_returns_latest_commit_at_or_before(index);
    }

    #[test]
    fn list_snapshots_contract() {
        let index = Arc::new(InMemoryMetaIndex::default());
        contract::list_snapshots_filters_and_orders(index);
    }

    #[test]
    fn list_tracked_files_contract() {
        let index = Arc::new(InMemoryMetaIndex::default());
        contract::list_tracked_files_excludes_deleted(index);
    }
}
