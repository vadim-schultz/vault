//! Read models for time-travel queries (`log`, `list`).

use super::change::FileEventKind;
use super::rel_path::RelPath;
use super::snapshot::CommitSha;

/// One snapshot entry returned by `log`, optionally scoped to a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotEntry {
    /// Commit object id.
    pub commit_sha: CommitSha,
    /// ISO-8601 UTC timestamp.
    pub created_at: String,
    /// Event kind for the queried path; `None` when `log` was not scoped to a path.
    pub event: Option<FileEventKind>,
}

/// A tracked file and the timestamp of its most recent non-delete snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedFile {
    /// Path relative to the vault worktree.
    pub path: RelPath,
    /// ISO-8601 UTC timestamp of the latest recorded change.
    pub last_modified: String,
}

/// One file's before/after content within a resolved commit, for diff/diffstat rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersionDiff {
    /// Path relative to the vault worktree.
    pub path: RelPath,
    /// Content before this change, or `None` when the path did not exist yet.
    pub previous: Option<Vec<u8>>,
    /// Content after this change, or `None` when the path was deleted.
    pub current: Option<Vec<u8>>,
}

/// A resolved commit's header message plus per-file diff content, ready for rendering.
///
/// Shared by `log` (one per historical commit) and `show`'s report mode (one, for the
/// resolved commit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReport {
    /// Header line, from the shared commit-message builder.
    pub message: String,
    /// Files touched by the commit (or by the query's path/prefix scope), in path order.
    pub files: Vec<FileVersionDiff>,
}
