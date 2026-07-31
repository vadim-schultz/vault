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
