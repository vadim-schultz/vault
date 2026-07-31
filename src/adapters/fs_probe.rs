//! Filesystem probe reporting what lives at a path.

use std::io::ErrorKind;
use std::path::Path;

use crate::domain::PathKind;
use crate::error::VaultError;

/// Inspect `abs` and report what the filesystem holds there.
///
/// A missing path yields [`PathKind::Missing`] rather than an error: deletions are
/// one of the reasons the watcher notified us about the path in the first place.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the path exists but cannot be inspected.
pub fn probe_path(abs: &Path) -> Result<PathKind, VaultError> {
    let metadata = match std::fs::metadata(abs) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(PathKind::Missing),
        Err(err) => return Err(VaultError::Io(err)),
    };
    if metadata.is_dir() {
        return Ok(PathKind::Directory);
    }
    if metadata.is_file() {
        return Ok(PathKind::File {
            size_bytes: metadata.len(),
        });
    }
    Ok(PathKind::Special)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn reports_missing_for_absent_path() {
        let dir = TempDir::new().expect("tempdir");
        let kind = probe_path(&dir.path().join("nope.md")).expect("probe");
        assert_eq!(kind, PathKind::Missing);
    }

    #[test]
    fn reports_directory_for_a_directory() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
        let kind = probe_path(&dir.path().join("sub")).expect("probe");
        assert_eq!(kind, PathKind::Directory);
    }

    #[test]
    fn reports_file_with_size() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("a.md");
        std::fs::write(&file, b"12345").expect("write");
        let kind = probe_path(&file).expect("probe");
        assert_eq!(kind, PathKind::File { size_bytes: 5 });
    }
}
