//! Relative path newtype with a single normalization rule.

use std::path::{Path, PathBuf};

use crate::error::VaultError;

/// A vault-relative path, normalized to forward slashes and UTF-8.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(String);

impl RelPath {
    /// Build a relative path from an absolute path under `worktree`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::PathOutsideWorktree`] when `abs` is not under `worktree`,
    /// or [`VaultError::NonUtf8Path`] when any component is not valid UTF-8.
    pub fn from_worktree(worktree: &Path, abs: &Path) -> Result<Self, VaultError> {
        let rel = abs
            .strip_prefix(worktree)
            .map_err(|_| VaultError::PathOutsideWorktree {
                path: abs.to_path_buf(),
            })?;
        Self::from_rel(rel)
    }

    /// Build from a path already relative to the vault worktree.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NonUtf8Path`] when any component is not valid UTF-8.
    pub fn from_rel(rel: &Path) -> Result<Self, VaultError> {
        let normalized = rel
            .components()
            .map(|c| {
                c.as_os_str()
                    .to_str()
                    .ok_or_else(|| VaultError::NonUtf8Path {
                        path: PathBuf::from(c.as_os_str()),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        Ok(Self(normalized))
    }

    /// Parse a stored path string (e.g. from `SQLite`).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        Self(s.to_string())
    }

    /// Return the normalized forward-slash path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to a platform `PathBuf` for filesystem operations.
    #[must_use]
    pub fn to_path(&self) -> PathBuf {
        self.0.split('/').collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_separators_to_forward_slash() {
        let rel = RelPath::from_rel(Path::new("sub/b.md")).expect("rel");
        assert_eq!(rel.as_str(), "sub/b.md");
    }

    #[test]
    fn rejects_path_outside_worktree() {
        let worktree = Path::new("/tmp/vault");
        let abs = Path::new("/other/file.md");
        let err = RelPath::from_worktree(worktree, abs).expect_err("outside");
        assert!(matches!(err, VaultError::PathOutsideWorktree { .. }));
    }

    #[test]
    fn from_worktree_strips_prefix() {
        let dir = std::env::temp_dir().join("vault-rel-test");
        let abs = dir.join("notes.md");
        let rel = RelPath::from_worktree(&dir, &abs).expect("rel");
        assert_eq!(rel.as_str(), "notes.md");
    }
}
