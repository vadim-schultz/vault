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
        .env("TZ", "UTC")
        .args(["show", "doc.md", "--at", "2026-06-02"])
        .assert()
        .success()
        .stdout("v2"); // end of 2026-06-02 (UTC, pinned above) resolves to that day's own commit

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

#[test]
fn show_with_no_path_prints_whole_vault_report() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"line1\n").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["show", "--at", "2026-06-02"])
        .assert()
        .success()
        .stdout(predicates::str::contains("update doc.md @"))
        .stdout(predicates::str::contains("+line1"));
}

#[test]
fn show_with_directory_path_scopes_report_to_subtree() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"top\n").expect("write");
    fs::create_dir_all(dir.path().join("sub")).expect("mkdir sub");
    fs::write(dir.path().join("sub/child.md"), b"nested\n").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    let output = common::vault_bin()
        .current_dir(dir.path())
        .args(["show", "sub", "--at", "2026-06-02"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf8");

    assert!(text.contains("sub/child.md"));
    assert!(!text.contains("+top"));
}

#[test]
fn show_single_file_path_is_unchanged_content_dump() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["show", "doc.md", "--at", "2026-06-02"])
        .assert()
        .success()
        .stdout("v1");
}
