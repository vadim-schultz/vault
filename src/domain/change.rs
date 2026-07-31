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

/// What the filesystem holds at a notified path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Nothing exists at the path.
    Missing,
    /// A directory.
    Directory,
    /// A regular file.
    File {
        /// Size on disk, in bytes.
        size_bytes: u64,
    },
    /// Neither a regular file nor a directory: socket, fifo, or device node.
    Special,
}

impl PathKind {
    /// Return the change this path represents, or `None` when it is not snapshottable.
    ///
    /// Directories are skipped deliberately. An event on a directory says nothing about
    /// the files inside it, and recording it as a delete would remove the entire subtree
    /// from the git tree.
    #[must_use]
    pub fn classify(self, max_file_bytes: u64) -> Option<FileEventKind> {
        match self {
            Self::Missing => Some(FileEventKind::Delete),
            Self::Directory | Self::Special => None,
            Self::File { size_bytes } if size_bytes > max_file_bytes => None,
            Self::File { .. } => Some(FileEventKind::Modify),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: u64 = 100;

    #[test]
    fn missing_path_is_a_delete() {
        assert_eq!(PathKind::Missing.classify(MAX), Some(FileEventKind::Delete));
    }

    #[test]
    fn directory_is_never_a_change() {
        assert_eq!(PathKind::Directory.classify(MAX), None);
    }

    #[test]
    fn special_file_is_never_a_change() {
        assert_eq!(PathKind::Special.classify(MAX), None);
    }

    #[test]
    fn regular_file_is_a_modify() {
        assert_eq!(
            PathKind::File { size_bytes: MAX }.classify(MAX),
            Some(FileEventKind::Modify)
        );
    }

    #[test]
    fn oversized_file_is_skipped() {
        assert_eq!(
            PathKind::File {
                size_bytes: MAX + 1
            }
            .classify(MAX),
            None
        );
    }
}
