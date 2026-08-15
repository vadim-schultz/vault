//! Integration tests for `vault reindex`.

mod common;

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use vault::paths::{GIT_DIR, META_DB, VAULT_DIR};

fn capture(dir: &Path, args: &[&str]) -> String {
    let output = common::vault_bin()
        .current_dir(dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).expect("utf8 stdout")
}

fn meta_db_path(dir: &Path) -> std::path::PathBuf {
    dir.join(VAULT_DIR).join(META_DB)
}

/// Read back the most recently inserted snapshot's `created_at`, straight from `meta.db` —
/// used to get an exact, real `--at` value for `vault restore` without guessing at wall-clock
/// timing between commits. Unlike [`common::backdate_last_snapshot`], this only reads; the
/// commit's own git message (which `vault reindex` recovers `created_at` from — see
/// `app::reindex`) must stay the source of truth for this test to mean anything, so nothing
/// here may diverge meta.db from it.
fn last_snapshot_time(dir: &Path) -> String {
    let conn = rusqlite::Connection::open(meta_db_path(dir)).expect("open meta.db");
    conn.query_row(
        "SELECT created_at FROM snapshots ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .expect("last snapshot time")
}

#[test]
fn reindex_rebuilds_meta_db_matching_original_log_and_show() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("a.md"), b"v1").expect("write a.md");
    fs::create_dir(dir.path().join("sub")).expect("mkdir sub");
    fs::write(dir.path().join("sub/deep.md"), b"nested").expect("write nested");
    common::init_in(dir.path());

    common::write_and_commit(dir.path(), "a.md", b"v2");
    let after_modify_a = last_snapshot_time(dir.path());

    common::write_and_commit(dir.path(), "c.md", b"new file");
    common::delete_and_commit(dir.path(), "a.md");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["restore", "a.md", "--at", &after_modify_a])
        .assert()
        .success();

    let before_log = capture(dir.path(), &["log"]);
    let before_show = capture(dir.path(), &["show", "--at", "2030-01-01"]);
    let before_list = capture(dir.path(), &["list"]);

    fs::remove_file(meta_db_path(dir.path())).expect("delete meta.db");

    common::vault_bin()
        .current_dir(dir.path())
        .arg("reindex")
        .assert()
        .success()
        .stdout(predicates::str::contains("Reindexed meta.db"))
        .stdout(predicates::str::contains("5 commits"));

    let after_log = capture(dir.path(), &["log"]);
    let after_show = capture(dir.path(), &["show", "--at", "2030-01-01"]);
    let after_list = capture(dir.path(), &["list"]);

    assert_eq!(before_log, after_log, "vault log must read identically");
    assert_eq!(before_show, after_show, "vault show must read identically");
    assert_eq!(before_list, after_list, "vault list must read identically");
}

#[test]
fn reindex_refuses_when_git_is_missing() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    let vault_dir = dir.path().join(VAULT_DIR);
    fs::create_dir_all(&vault_dir).expect("mkdir vault");
    vault::storage::sqlite::init_meta_db(&vault_dir.join(META_DB)).expect("meta.db init");
    fs::write(vault_dir.join(vault::paths::README_FILE), b"x").expect("readme");
    fs::write(vault_dir.join(vault::paths::CONFIG_FILE), b"x").expect("config");

    common::vault_bin()
        .current_dir(dir.path())
        .arg("reindex")
        .assert()
        .failure()
        .stderr(predicates::str::contains(GIT_DIR));
}

#[test]
fn reindex_refuses_populated_meta_db_without_force_then_succeeds_with_it() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("a.md"), b"v1").expect("write");
    common::init_in(dir.path());

    common::vault_bin()
        .current_dir(dir.path())
        .arg("reindex")
        .assert()
        .failure()
        .stderr(predicates::str::contains("--force"));

    common::vault_bin()
        .current_dir(dir.path())
        .args(["reindex", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Reindexed meta.db"));
}

#[test]
fn reindex_dry_run_does_not_modify_meta_db() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("a.md"), b"v1").expect("write");
    common::init_in(dir.path());
    let before = fs::read(meta_db_path(dir.path())).expect("read meta.db before");

    common::vault_bin()
        .current_dir(dir.path())
        .args(["reindex", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Would reindex"))
        .stdout(predicates::str::contains("--force"));

    let after = fs::read(meta_db_path(dir.path())).expect("read meta.db after");
    assert_eq!(before, after, "dry run must not touch meta.db");
    assert!(
        !dir.path()
            .join(VAULT_DIR)
            .join(format!("{META_DB}.reindex.tmp"))
            .exists(),
        "dry run must not leave a temp file behind"
    );
}
