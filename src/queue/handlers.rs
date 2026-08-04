//! Task handlers for the background work queue.

use std::collections::HashSet;
use std::path::Path;

use crate::adapters::SqliteMetaIndex;
use crate::config::VaultConfig;
use crate::daemon;
use crate::domain::{TaskKind, VaultLayout};
use crate::error::VaultError;
use crate::ports::MetaIndex;
use crate::walk::collect_baseline_changes;

/// Dispatch `kind` to its handler.
///
/// # Errors
///
/// Returns [`VaultError`] when the handler fails.
pub fn run(kind: &TaskKind) -> Result<(), VaultError> {
    match kind {
        TaskKind::ReconcileWalk { vault_root, .. } => reconcile_walk(vault_root),
    }
}

/// Walk watch roots and diff against tracked files, logging mismatches.
///
/// # Errors
///
/// Returns [`VaultError`] when vault metadata cannot be read.
pub fn reconcile_walk(vault_root: &Path) -> Result<(), VaultError> {
    let layout = VaultLayout::from_worktree(vault_root.to_path_buf());
    let config = VaultConfig::load(&layout.config_path())?;
    let disk_changes = collect_baseline_changes(&layout, &config)?;
    let disk_paths: HashSet<_> = disk_changes.iter().map(|c| c.rel.as_str()).collect();

    let index = SqliteMetaIndex::open(layout.meta_db_path())?;
    let tracked = index.list_tracked_files()?;
    let tracked_paths: HashSet<_> = tracked.iter().map(|t| t.path.as_str()).collect();

    let untracked_on_disk = disk_paths.difference(&tracked_paths).count();
    let missing_on_disk = tracked_paths.difference(&disk_paths).count();
    let mismatch_count = untracked_on_disk + missing_on_disk;

    if mismatch_count > 0 {
        daemon::append_log(&format!(
            "reconcile_walk {}: {mismatch_count} file(s) mismatch \
             ({untracked_on_disk} untracked on disk, {missing_on_disk} missing on disk)",
            vault_root.display(),
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::adapters::{GixObjectStore, SqliteMetaIndex, SystemClock};
    use crate::app::snapshot;
    use crate::domain::{FileChange, FileEventKind, RelPath};
    use crate::paths::{daemon_log_path, STATE_DIR_ENV};
    use crate::storage;

    fn init_vault(dir: &TempDir) -> VaultLayout {
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git init");
        storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite init");
        VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("write config");
        fs::write(layout.readme_path(), b"test").expect("readme");
        layout
    }

    #[test]
    fn reconcile_walk_silent_when_sets_match() {
        let _guard = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let state_dir = TempDir::new().expect("state tempdir");
        std::env::set_var(STATE_DIR_ENV, state_dir.path());

        let dir = TempDir::new().expect("vault tempdir");
        let layout = init_vault(&dir);
        fs::write(layout.worktree.join("a.md"), b"a").expect("write");
        let changes = vec![FileChange {
            rel: RelPath::parse("a.md"),
            kind: FileEventKind::Create,
        }];
        let object_store = GixObjectStore::open(&layout).expect("git");
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("meta");
        snapshot::commit(&layout, &changes, &SystemClock, &object_store, &meta_index)
            .expect("commit");

        reconcile_walk(&layout.worktree).expect("reconcile");

        let log_path = daemon_log_path().expect("log path");
        if log_path.is_file() {
            let contents = fs::read_to_string(log_path).expect("read log");
            assert!(!contents.contains("mismatch"));
        }

        std::env::remove_var(STATE_DIR_ENV);
    }

    #[test]
    fn reconcile_walk_logs_mismatch_for_untracked_file() {
        let _guard = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let state_dir = TempDir::new().expect("state tempdir");
        std::env::set_var(STATE_DIR_ENV, state_dir.path());

        let dir = TempDir::new().expect("vault tempdir");
        let layout = init_vault(&dir);
        fs::write(layout.worktree.join("orphan.md"), b"orphan").expect("write");

        reconcile_walk(&layout.worktree).expect("reconcile");

        let log_path = daemon_log_path().expect("log path");
        let contents = fs::read_to_string(log_path).expect("read log");
        assert!(contents.contains("1 file(s) mismatch"));
        assert!(contents.contains("1 untracked on disk"));

        std::env::remove_var(STATE_DIR_ENV);
    }
}
