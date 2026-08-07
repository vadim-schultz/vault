//! Integration tests for the global vault registry.

mod common;

use std::fs;

use rusqlite::Connection;
use tempfile::TempDir;
use vault::paths::{META_DB, VAULT_DIR};
use vault::registry::VaultRegistry;

#[test]
fn init_registers_vault_root() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    let registry = VaultRegistry::load().expect("load");
    assert_eq!(registry.vault.len(), 1);
    assert_eq!(
        registry.vault[0].root,
        dir.path().canonicalize().expect("canon")
    );
}

#[test]
fn two_inits_produce_two_entries() {
    let _env = common::VaultEnv::new();
    let dir_a = TempDir::new().expect("tempdir a");
    let dir_b = TempDir::new().expect("tempdir b");
    common::init_in(dir_a.path());
    common::init_in(dir_b.path());

    let registry = VaultRegistry::load().expect("load");
    assert_eq!(registry.vault.len(), 2);
}

#[test]
fn second_init_does_not_duplicate_registry_entry() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());
    common::vault_bin()
        .env(vault::paths::NO_SERVICE_ENV, "1")
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    let registry = VaultRegistry::load().expect("load");
    assert_eq!(registry.vault.len(), 1);
}

#[test]
fn registry_stays_under_state_dir() {
    let env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    let registry_file = env.state_path().join("registry.toml");
    assert!(registry_file.is_file());
}

#[test]
fn init_creates_baseline_snapshot() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("notes.md"), b"hello").expect("write");
    common::init_in(dir.path());

    let db_path = dir.path().join(VAULT_DIR).join(META_DB);
    let conn = Connection::open(db_path).expect("open");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 1);
}
