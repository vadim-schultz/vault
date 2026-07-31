//! Integration tests for `vault status`.

mod common;

use std::fs;

use tempfile::TempDir;
use vault::paths::{META_DB, VAULT_DIR};

#[test]
fn status_reports_stopped_daemon_and_registered_vault() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("notes.md"), b"v1").expect("write");
    common::init_in(dir.path());

    common::vault_bin()
        .env(vault::paths::NO_SERVICE_ENV, "1")
        .arg("status")
        .assert()
        .success()
        .stdout(predicates::str::contains("Daemon: stopped"))
        .stdout(predicates::str::contains("Vaults: 1"))
        .stdout(predicates::str::contains("last snapshot:"));
}

#[test]
fn status_shows_baseline_snapshot_time() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("doc.md"), b"content").expect("write");
    common::init_in(dir.path());

    let db_path = dir.path().join(VAULT_DIR).join(META_DB);
    let last = vault::storage::sqlite::last_snapshot_time(&db_path).expect("time");
    let last = last.expect("some snapshot");

    common::vault_bin()
        .env(vault::paths::NO_SERVICE_ENV, "1")
        .arg("status")
        .assert()
        .success()
        .stdout(predicates::str::contains(&last));
}

#[test]
fn status_does_not_write_registry() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("notes.md"), b"v1").expect("write");
    common::init_in(dir.path());

    let registry = vault::paths::registry_path().expect("registry path");
    let mtime_before = fs::metadata(&registry)
        .expect("metadata")
        .modified()
        .expect("mtime");

    common::vault_bin()
        .env(vault::paths::NO_SERVICE_ENV, "1")
        .arg("status")
        .assert()
        .success();

    let mtime_after = fs::metadata(&registry)
        .expect("metadata")
        .modified()
        .expect("mtime");
    assert_eq!(mtime_before, mtime_after);
}
