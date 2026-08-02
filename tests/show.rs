//! Integration tests for `vault show`.

mod common;

use std::fs;

use tempfile::TempDir;

#[test]
fn show_returns_content_at_or_before_date() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    common::snapshot_at(dir.path(), "doc.md", b"v2", "2026-06-02T09:00:00+00:00");
    common::snapshot_at(dir.path(), "doc.md", b"v3", "2026-06-03T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["show", "doc.md", "--at", "2026-06-02"])
        .assert()
        .success()
        .stdout("v1"); // 2026-06-02 UTC midnight resolves to the 06-01 09:00 commit

    common::vault_bin()
        .current_dir(dir.path())
        .args(["show", "doc.md", "--at", "2026-06-02T12:00:00+00:00"])
        .assert()
        .success()
        .stdout("v2");
}

#[test]
fn show_before_any_snapshot_fails_clearly() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["show", "doc.md", "--at", "2020-01-01"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no snapshot at or before"));
}

#[test]
fn show_untracked_path_fails_clearly() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["show", "missing.md", "--at", "2026-06-02"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not tracked"));
}
