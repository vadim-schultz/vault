#![allow(clippy::missing_errors_doc)]

//! `SQLite` metadata index port.

use crate::domain::{CommitSha, RelPath, SnapshotEntry, SnapshotRecord, TrackedFile};
use crate::error::VaultError;

/// Time-based metadata index.
pub trait MetaIndex: Send + Sync {
    /// Record a snapshot and its file events.
    fn record_snapshot(&self, record: &SnapshotRecord) -> Result<(), VaultError>;

    /// Return the latest snapshot timestamp.
    fn last_snapshot_time(&self) -> Result<Option<String>, VaultError>;

    /// Resolve the latest commit at or before `at` (ISO-8601).
    fn resolve_at(&self, at: &str) -> Result<Option<CommitSha>, VaultError>;

    /// List snapshots, optionally scoped to `path`, newest first.
    fn list_snapshots(&self, path: Option<&RelPath>) -> Result<Vec<SnapshotEntry>, VaultError>;

    /// List tracked files whose latest event is not a delete, ordered by path.
    fn list_tracked_files(&self) -> Result<Vec<TrackedFile>, VaultError>;
}

#[cfg(test)]
pub mod contract {
    use super::*;
    use crate::domain::{FileChange, FileEventKind};
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

    /// List snapshots filters and orders correctly.
    pub fn list_snapshots_filters_and_orders(index: Arc<dyn MetaIndex>) {
        let changes_a_create = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        let changes_b_create = vec![FileChange {
            rel: RelPath::parse("b.md"),
            kind: FileEventKind::Create,
        }];
        let changes_a_modify = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Modify,
        }];

        index
            .record_snapshot(&SnapshotRecord {
                commit_sha: CommitSha("snap1".to_string()),
                created_at: "2026-01-01T10:00:00Z".to_string(),
                changes: changes_a_create,
            })
            .expect("insert 1");

        index
            .record_snapshot(&SnapshotRecord {
                commit_sha: CommitSha("snap2".to_string()),
                created_at: "2026-01-02T10:00:00Z".to_string(),
                changes: changes_b_create,
            })
            .expect("insert 2");

        index
            .record_snapshot(&SnapshotRecord {
                commit_sha: CommitSha("snap3".to_string()),
                created_at: "2026-01-03T10:00:00Z".to_string(),
                changes: changes_a_modify,
            })
            .expect("insert 3");

        let all = index.list_snapshots(None).expect("list all");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].commit_sha.0, "snap3");
        assert_eq!(all[1].commit_sha.0, "snap2");
        assert_eq!(all[2].commit_sha.0, "snap1");

        let a_only = index.list_snapshots(Some(&RelPath::parse("a.md"))).expect("list a.md");
        assert_eq!(a_only.len(), 2);
        assert_eq!(a_only[0].commit_sha.0, "snap3");
        assert_eq!(a_only[0].event, Some(FileEventKind::Modify));
        assert_eq!(a_only[1].commit_sha.0, "snap1");
        assert_eq!(a_only[1].event, Some(FileEventKind::Create));
    }

    /// List tracked files excludes deleted paths.
    pub fn list_tracked_files_excludes_deleted(index: Arc<dyn MetaIndex>) {
        let changes_create = vec![
            FileChange {
                rel: RelPath::parse("a.md"),
                kind: FileEventKind::Create,
            },
            FileChange {
                rel: RelPath::parse("b.md"),
                kind: FileEventKind::Create,
            },
        ];

        index
            .record_snapshot(&SnapshotRecord {
                commit_sha: CommitSha("snap1".to_string()),
                created_at: "2026-01-01T10:00:00Z".to_string(),
                changes: changes_create,
            })
            .expect("insert 1");

        let changes_delete_b = vec![FileChange {
            rel: RelPath::parse("b.md"),
            kind: FileEventKind::Delete,
        }];

        index
            .record_snapshot(&SnapshotRecord {
                commit_sha: CommitSha("snap2".to_string()),
                created_at: "2026-01-02T10:00:00Z".to_string(),
                changes: changes_delete_b,
            })
            .expect("insert 2");

        let tracked = index.list_tracked_files().expect("list tracked");
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked[0].path.as_str(), "a.md");
    }
}
