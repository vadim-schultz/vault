//! Saves the process cwd on entry and restores it on drop.

use std::path::Path;

use crate::error::VaultError;

/// Saves the process cwd on entry and restores it on drop; uses `worktree` when cwd is unavailable.
pub(crate) struct WorktreeCwd<'a> {
    restore: Option<std::path::PathBuf>,
    _worktree: &'a Path,
}

impl<'a> WorktreeCwd<'a> {
    pub(crate) fn enter(worktree: &'a Path) -> Result<Self, VaultError> {
        let restore = if let Ok(current) = std::env::current_dir() {
            Some(current)
        } else {
            std::env::set_current_dir(worktree)?;
            None
        };
        Ok(Self {
            restore,
            _worktree: worktree,
        })
    }
}

impl Drop for WorktreeCwd<'_> {
    fn drop(&mut self) {
        if let Some(dir) = self.restore.take() {
            let _ = std::env::set_current_dir(dir);
        }
    }
}
