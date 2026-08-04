//! Integration tests for `vault diff`.

mod common;

use std::fs;

use tempfile::TempDir;

#[test]
fn diff_between_two_snapshots_shows_unified_diff() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"line1\n").expect("write");
    common::init_in(dir.path());
    common::backdate_last_snapshot(dir.path(), "2026-06-01T09:00:00+00:00");
    common::snapshot_at(
        dir.path(),
        "doc.md",
        b"line2\n",
        "2026-06-02T09:00:00+00:00",
    );

    common::vault_bin()
        .current_dir(dir.path())
        .args([
            "diff",
            "doc.md",
            "--at",
            "2026-06-01T09:00:00+00:00",
            "--to",
            "2026-06-02T09:00:00+00:00",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("-line1"))
        .stdout(predicates::str::contains("+line2"));
}

#[test]
fn diff_with_no_flags_compares_last_snapshot_to_working_tree() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"line1\n").expect("write");
    common::init_in(dir.path());

    fs::write(dir.path().join("doc.md"), b"line1\nline2\n").expect("edit without committing");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["diff", "doc.md"])
        .assert()
        .success()
        .stdout(predicates::str::contains("+line2"));
}

#[test]
fn diff_to_without_at_is_a_usage_error() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"line1\n").expect("write");
    common::init_in(dir.path());

    common::vault_bin()
        .current_dir(dir.path())
        .args(["diff", "doc.md", "--to", "2026-06-02"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "the following required arguments were not provided",
        ))
        .stderr(predicates::str::contains("--at <AT>"));
}
