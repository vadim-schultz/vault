//! Integration tests for `vault restore`.

mod common;

use std::fs;

use tempfile::TempDir;

#[test]
fn restore_writes_content_and_records_its_own_snapshot() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");
    common::snapshot_at(dir.path(), "doc.md", b"v2", "2026-06-02T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["restore", "doc.md", "--at", "2026-06-01T09:00:00+00:00"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Restored doc.md"));

    assert_eq!(fs::read(dir.path().join("doc.md")).expect("read"), b"v1");

    let log_output = common::vault_bin()
        .current_dir(dir.path())
        .args(["log", "doc.md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(log_output).expect("utf8");
    assert!(text
        .lines()
        .next()
        .expect("newest line")
        .contains("restore"));
}

#[test]
fn restore_dry_run_leaves_working_tree_and_history_untouched() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");
    common::snapshot_at(dir.path(), "doc.md", b"v3", "2026-06-03T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args([
            "restore",
            "doc.md",
            "--at",
            "2026-06-01T09:00:00+00:00",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("dry run"));

    assert_eq!(fs::read(dir.path().join("doc.md")).expect("read"), b"v3");

    let log_output = common::vault_bin()
        .current_dir(dir.path())
        .args(["log", "doc.md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        header_count(&String::from_utf8(log_output).expect("utf8")),
        2 // baseline create + the one modify, no restore entry
    );
}

#[test]
fn restoring_the_current_version_is_a_no_op() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["restore", "doc.md", "--at", "2026-06-01T09:00:00+00:00"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already matches that version"));

    let log_output = common::vault_bin()
        .current_dir(dir.path())
        .args(["log", "doc.md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        header_count(&String::from_utf8(log_output).expect("utf8")),
        1 // just the baseline create, no-op restore added nothing
    );
}

fn header_count(log_text: &str) -> usize {
    log_text
        .lines()
        .filter(|line| {
            line.starts_with("update")
                || line.starts_with("delete")
                || line.starts_with("restore")
                || line.starts_with("change")
        })
        .count()
}

#[test]
fn restoring_a_time_with_no_snapshot_fails_and_writes_nothing() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["restore", "doc.md", "--at", "2020-01-01"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no snapshot at or before"));

    assert_eq!(fs::read(dir.path().join("doc.md")).expect("read"), b"v1");
}
