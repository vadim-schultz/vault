//! Ignore-glob matching for watched paths.

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::config::VaultConfig;
use crate::domain::RelPath;
use crate::error::VaultError;

/// Compiled ignore matcher for a vault configuration.
#[derive(Clone, Debug)]
pub struct IgnoreMatcher {
    set: GlobSet,
}

impl IgnoreMatcher {
    /// Build a matcher from `config.ignore` patterns.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidGlob`] when a glob pattern is invalid.
    pub fn from_config(config: &VaultConfig) -> Result<Self, VaultError> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &config.ignore {
            let glob = Glob::new(pattern).map_err(|e| VaultError::InvalidGlob {
                pattern: format!("{pattern}: {e}"),
            })?;
            builder.add(glob);
        }
        let set = builder.build().map_err(|e| VaultError::InvalidGlob {
            pattern: e.to_string(),
        })?;
        Ok(Self { set })
    }

    /// Return whether `rel_path` (relative to the vault root) is ignored.
    #[must_use]
    pub fn is_ignored(&self, rel_path: &RelPath) -> bool {
        self.set.is_match(rel_path.as_str())
    }

    /// Return whether a raw relative path is ignored.
    #[must_use]
    pub fn is_ignored_path(&self, rel_path: &Path) -> bool {
        RelPath::from_rel(rel_path).map_or(true, |rel| self.is_ignored(&rel))
    }
}

/// Return whether `abs_path` exceeds `max_bytes` on disk.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when metadata cannot be read.
pub fn exceeds_max_bytes(abs_path: &Path, max_bytes: u64) -> Result<bool, VaultError> {
    if !abs_path.is_file() {
        return Ok(false);
    }
    let len = abs_path.metadata()?.len();
    Ok(len > max_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_vault_glob() {
        let config = VaultConfig::defaults();
        let matcher = IgnoreMatcher::from_config(&config).expect("matcher");
        let rel = RelPath::parse(".vault/meta.db");
        assert!(matcher.is_ignored(&rel));
        assert!(!matcher.is_ignored(&RelPath::parse("notes.md")));
    }

    #[test]
    fn ignores_swap_files() {
        let config = VaultConfig::defaults();
        let matcher = IgnoreMatcher::from_config(&config).expect("matcher");
        assert!(matcher.is_ignored(&RelPath::parse("draft.md.swp")));
    }

    #[test]
    fn rejects_invalid_glob_pattern() {
        let mut config = VaultConfig::defaults();
        config.ignore.push("[unclosed".to_string());

        let err = IgnoreMatcher::from_config(&config).expect_err("invalid glob");
        assert!(matches!(err, VaultError::InvalidGlob { .. }));
    }
}
