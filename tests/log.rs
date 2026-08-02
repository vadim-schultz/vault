//! Integration tests for `vault log`.

mod common;

use std::fs;

use tempfile::TempDir;

#[test]
fn log_lists_all_snapshots_newest_first() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");
    common::snapshot_at(dir.path(), "doc.md", b"v2", "2026-06-02T09:00:00+00:00");
    common::snapshot_at(dir.path(), "doc.md", b"v3", "2026-06-03T09:00:00+00:00");

    let output = common::vault_bin()
        .current_dir(dir.path())
        .arg("log")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf8");
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("2026-06-03T09:00:00"));
    assert!(lines[1].contains("2026-06-02T09:00:00"));
    assert!(lines[2].contains("2026-06-01T09:00:00"));
}

#[test]
fn log_scoped_to_path_excludes_other_files() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"v1").expect("write");
    fs::write(dir.path().join("other.md"), b"o1").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");
    common::snapshot_at(dir.path(), "doc.md", b"v2", "2026-06-02T09:00:00+00:00");

    let output = common::vault_bin()
        .current_dir(dir.path())
        .args(["log", "doc.md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf8");

    assert!(text.contains("modify"));
    assert_eq!(text.lines().count(), 2); // baseline create + modify, not other.md's create
}

#[test]
fn log_on_empty_vault_reports_no_snapshots() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    common::vault_bin()
        .current_dir(dir.path())
        .arg("log")
        .assert()
        .success()
        .stdout(predicates::str::contains("No snapshots yet"));
}
