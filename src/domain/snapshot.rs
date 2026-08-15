//! Snapshot domain types.

/// Git commit object id (hex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSha(pub String);

impl CommitSha {
    /// Return the hex SHA string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Result of a successful snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotResult {
    /// Commit object id.
    pub commit_sha: CommitSha,
    /// ISO-8601 UTC timestamp.
    pub created_at: String,
}

/// Snapshot row for the metadata index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRecord {
    /// Commit object id.
    pub commit_sha: CommitSha,
    /// ISO-8601 UTC timestamp.
    pub created_at: String,
    /// File changes in this snapshot.
    pub changes: Vec<super::change::FileChange>,
}

/// One commit as observed by walking `.git`'s history directly, for `vault reindex`.
///
/// Unlike [`SnapshotRecord`], `created_at` isn't known yet — it (and, for a restore, the exact
/// `Restore` classification) still needs to be recovered from `message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCommit {
    /// Commit object id.
    pub sha: CommitSha,
    /// Raw git commit message, as written by `app::snapshot::commit`.
    pub message: String,
    /// File changes, derived from diffing this commit's tree against its parent's.
    pub changes: Vec<super::change::FileChange>,
    /// This commit's own committer timestamp (ISO-8601 UTC), as a fallback `created_at` for
    /// messages that don't match vault's own `"... @ <created_at>"` format. Lower fidelity than
    /// parsing the message — see `app::reindex`'s design notes.
    pub committer_time: String,
}
