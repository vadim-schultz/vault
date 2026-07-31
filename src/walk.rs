//! Walk vault worktrees to collect baseline file paths.

use crate::config::VaultConfig;
use crate::domain::{FileChange, FileEventKind, RelPath, VaultLayout};
use crate::error::VaultError;
use crate::ignore::{exceeds_max_bytes, IgnoreMatcher};
use walkdir::{DirEntry, WalkDir};

/// Collect non-ignored files under `layout` for a baseline snapshot.
///
/// # Errors
///
/// Returns [`VaultError`] when walking fails or a file exceeds size limits.
pub fn collect_baseline_changes(
    layout: &VaultLayout,
    config: &VaultConfig,
) -> Result<Vec<FileChange>, VaultError> {
    let matcher = IgnoreMatcher::from_config(config)?;
    let mut changes = Vec::new();
    for root in &config.watch_roots {
        let watch_root = layout.worktree.join(root);
        collect_from_watch_root(
            &watch_root,
            &layout.worktree,
            &matcher,
            config.watcher.max_file_bytes,
            &mut changes,
        )?;
    }
    Ok(changes)
}

fn collect_from_watch_root(
    watch_root: &std::path::Path,
    worktree: &std::path::Path,
    matcher: &IgnoreMatcher,
    max_file_bytes: u64,
    changes: &mut Vec<FileChange>,
) -> Result<(), VaultError> {
    if !watch_root.is_dir() {
        return Ok(());
    }
    for entry in WalkDir::new(watch_root).follow_links(false) {
        let entry = entry.map_err(|e| VaultError::Io(e.into()))?;
        if let Some(change) = file_change_from_entry(&entry, worktree, matcher, max_file_bytes)? {
            changes.push(change);
        }
    }
    Ok(())
}

fn file_change_from_entry(
    entry: &DirEntry,
    worktree: &std::path::Path,
    matcher: &IgnoreMatcher,
    max_file_bytes: u64,
) -> Result<Option<FileChange>, VaultError> {
    if !entry.file_type().is_file() {
        return Ok(None);
    }
    let abs = entry.path();
    let rel = RelPath::from_worktree(worktree, abs)?;
    if matcher.is_ignored(&rel) || exceeds_max_bytes(abs, max_file_bytes)? {
        return Ok(None);
    }
    Ok(Some(FileChange {
        rel,
        kind: FileEventKind::Create,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn layout(dir: &TempDir) -> VaultLayout {
        VaultLayout::from_worktree(dir.path().to_path_buf())
    }

    #[test]
    fn collects_files_recursively() {
        let dir = TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join("sub")).expect("mkdir sub");
        fs::write(dir.path().join("a.md"), b"a").expect("write a");
        fs::write(dir.path().join("sub").join("b.md"), b"b").expect("write b");
        let config = VaultConfig::defaults();

        let changes = collect_baseline_changes(&layout(&dir), &config).expect("collect");

        let rels: Vec<_> = changes.iter().map(|c| c.rel.as_str()).collect();
        assert!(rels.contains(&"a.md"));
        assert!(rels.contains(&"sub/b.md"));
        assert!(changes.iter().all(|c| c.kind == FileEventKind::Create));
    }

    #[test]
    fn skips_ignored_patterns() {
        let dir = TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join(".vault")).expect("mkdir .vault");
        fs::write(dir.path().join(".vault").join("config.toml"), b"x").expect("write config");
        fs::write(dir.path().join("draft.md.swp"), b"x").expect("write swp");
        fs::write(dir.path().join("keep.md"), b"keep").expect("write keep");
        let config = VaultConfig::defaults();

        let changes = collect_baseline_changes(&layout(&dir), &config).expect("collect");

        let rels: Vec<_> = changes.iter().map(|c| c.rel.as_str()).collect();
        assert_eq!(rels, vec!["keep.md"]);
    }

    #[test]
    fn skips_oversized_files() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("big.bin"), vec![0_u8; 20]).expect("write big");
        fs::write(dir.path().join("small.md"), b"ok").expect("write small");
        let mut config = VaultConfig::defaults();
        config.watcher.max_file_bytes = 10;

        let changes = collect_baseline_changes(&layout(&dir), &config).expect("collect");

        let rels: Vec<_> = changes.iter().map(|c| c.rel.as_str()).collect();
        assert_eq!(rels, vec!["small.md"]);
    }

    #[test]
    fn missing_watch_root_yields_no_changes() {
        let dir = TempDir::new().expect("tempdir");
        let mut config = VaultConfig::defaults();
        config.watch_roots = vec!["does-not-exist".to_string()];

        let changes = collect_baseline_changes(&layout(&dir), &config).expect("collect");

        assert!(changes.is_empty());
    }

    #[test]
    fn walks_each_configured_watch_root() {
        let dir = TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join("notes")).expect("mkdir notes");
        fs::create_dir_all(dir.path().join("docs")).expect("mkdir docs");
        fs::write(dir.path().join("notes").join("a.md"), b"a").expect("write a");
        fs::write(dir.path().join("docs").join("b.md"), b"b").expect("write b");
        let mut config = VaultConfig::defaults();
        config.watch_roots = vec!["notes".to_string(), "docs".to_string()];

        let changes = collect_baseline_changes(&layout(&dir), &config).expect("collect");

        let rels: Vec<_> = changes.iter().map(|c| c.rel.as_str()).collect();
        assert!(rels.contains(&"notes/a.md"));
        assert!(rels.contains(&"docs/b.md"));
    }
}
