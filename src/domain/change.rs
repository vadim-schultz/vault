//! File change types for snapshot commits.

use super::rel_path::RelPath;

/// Kind of file change recorded in `file_events`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventKind {
    /// File was created.
    Create,
    /// File was modified.
    Modify,
    /// File was deleted.
    Delete,
}

impl FileEventKind {
    /// Return the sqlite `event_type` string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
        }
    }
}

/// One file change to include in a snapshot commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path relative to the vault worktree.
    pub rel: RelPath,
    /// Change kind.
    pub kind: FileEventKind,
}
