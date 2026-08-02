//! Integration tests for `vault list`.

mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::PredicateBooleanExt;
use tempfile::TempDir;

/// Delete `rel` from disk and commit the deletion via the real snapshot pipeline
/// (bypassing the watcher's debounce), mirroring `common::write_and_commit`.
fn remove_and_commit(worktree: &Path, rel: &str) {
    fs::remove_file(worktree.join(rel)).expect("remove");
    let vault = vault::watcher::router::WatchedVault::load(worktree).expect("load vault");
    vault::watcher::worker::commit_batch(&vault, &[vault::domain::RelPath::parse(rel)])
        .expect("commit delete");
}

#[test]
fn list_excludes_deleted_files() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("keep.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::write_and_commit(dir.path(), "gone.md", b"v1");

    remove_and_commit(dir.path(), "gone.md");

    common::vault_bin()
        .current_dir(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("keep.md"))
        .stdout(predicates::str::contains("gone.md").not());
}

#[test]
fn list_on_empty_vault_reports_no_tracked_files() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    common::vault_bin()
        .current_dir(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("No tracked files"));
}
