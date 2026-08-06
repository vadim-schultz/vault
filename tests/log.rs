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
    let headers: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("update") || line.starts_with("change"))
        .collect();

    assert_eq!(headers.len(), 3);
    assert!(headers[0].contains("2026-06-03T09:00:00"));
    assert!(headers[1].contains("2026-06-02T09:00:00"));
    assert!(headers[2].contains("2026-06-01T09:00:00"));
    assert!(text.lines().next().unwrap().starts_with("update doc.md @"));
    for line in text.lines() {
        assert!(
            !is_bare_hex_sha(line),
            "line looks like a bare commit hash: {line:?}"
        );
    }
}

fn is_bare_hex_sha(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    first.len() >= 7 && first.chars().all(|c| c.is_ascii_hexdigit())
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

    let headers: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("update") || line.starts_with("change"))
        .collect();
    assert_eq!(headers.len(), 2); // baseline (scoped) + modify, not other.md's own history
    assert!(!text.contains("other.md"));
    assert!(text.contains(" doc.md | 2 +-\n")); // v1 -> v2 modify
    assert!(text.contains(" doc.md | 1 +\n")); // baseline create, scoped to doc.md only
}

#[test]
fn log_verbose_shows_full_diff_hunks_no_hash() {
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
        .args(["--verbose", "log", "doc.md"])
        .assert()
        .success()
        .stdout(predicates::str::contains("-line1"))
        .stdout(predicates::str::contains("+line2"));
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
