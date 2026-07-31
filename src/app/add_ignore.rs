//! `vault ignore` use-case.

use crate::config::VaultConfig;
use crate::domain::VaultLayout;
use crate::error::VaultError;

/// Append an ignore pattern to a vault config when not already present.
///
/// # Errors
///
/// Returns [`VaultError`] when the config cannot be loaded or written.
pub fn add_pattern(layout: &VaultLayout, pattern: &str) -> Result<bool, VaultError> {
    let config_path = layout.config_path();
    let mut config = VaultConfig::load(&config_path)?;
    if !config.add_ignore(pattern) {
        return Ok(false);
    }
    config.write_to(&config_path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn duplicate_pattern_does_not_rewrite_file() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir");
        let config = VaultConfig::defaults();
        config.write_to(&layout.config_path()).expect("write");
        let mtime_before = fs::metadata(&layout.config_path())
            .expect("meta")
            .modified()
            .expect("mtime");

        assert!(!add_pattern(&layout, ".vault/**").expect("add"));

        let mtime_after = fs::metadata(&layout.config_path())
            .expect("meta")
            .modified()
            .expect("mtime");
        assert_eq!(mtime_before, mtime_after);
    }
}
