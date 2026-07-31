#![allow(clippy::missing_errors_doc)]

//! `SQLite` metadata index port.

use crate::domain::{CommitSha, SnapshotRecord};
use crate::error::VaultError;

/// Time-based metadata index.
pub trait MetaIndex: Send + Sync {
    /// Record a snapshot and its file events.
    fn record_snapshot(&self, record: &SnapshotRecord) -> Result<(), VaultError>;

    /// Return the latest snapshot timestamp.
    fn last_snapshot_time(&self) -> Result<Option<String>, VaultError>;

    /// Resolve the latest commit at or before `at` (ISO-8601).
    fn resolve_at(&self, at: &str) -> Result<Option<CommitSha>, VaultError>;
}

#[cfg(test)]
pub mod contract {
    use super::*;
    use std::sync::Arc;

    /// Shared contract test for any `MetaIndex` implementation.
    pub fn resolve_at_returns_latest_commit_at_or_before(index: Arc<dyn MetaIndex>) {
        for (sha, at) in [
            ("aaa", "2026-01-01T10:00:00Z"),
            ("bbb", "2026-01-02T10:00:00Z"),
        ] {
            index
                .record_snapshot(&SnapshotRecord {
                    commit_sha: CommitSha(sha.to_string()),
                    created_at: at.to_string(),
                    changes: vec![],
                })
                .expect("insert");
        }
        assert_eq!(
            index
                .resolve_at("2026-01-01T12:00:00Z")
                .expect("resolve")
                .map(|s| s.0),
            Some("aaa".to_string())
        );
        assert_eq!(
            index
                .resolve_at("2026-01-03T00:00:00Z")
                .expect("resolve")
                .map(|s| s.0),
            Some("bbb".to_string())
        );
    }
}
