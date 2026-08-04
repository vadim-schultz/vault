//! Integration tests for `vault init`.

mod common;

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use tempfile::TempDir;
use vault::paths::{CONFIG_FILE, GIT_DIR, META_DB, VAULT_DIR};
use vault::storage::sqlite::{
    COUNT_INDEX_BY_NAME, IDX_FILE_EVENTS_PATH_TIME, IDX_SNAPSHOTS_CREATED_AT,
};

#[test]
fn init_creates_vault_layout() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());
    common::assert_vault_layout(dir.path());
    common::assert_no_root_git(dir.path());
}

#[test]
fn init_rejects_second_run() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());
    common::vault_bin()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicates::str::contains("already initialized"));
}

#[test]
fn init_does_not_touch_root_git() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    let git_dir = dir.path().join(GIT_DIR);
    fs::create_dir(&git_dir).expect("create root .git");
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").expect("write HEAD");

    common::init_in(dir.path());

    assert!(git_dir.is_dir());
    assert_eq!(
        fs::read_to_string(git_dir.join("HEAD")).expect("read HEAD"),
        "ref: refs/heads/main\n"
    );
    common::assert_vault_layout(dir.path());
}

#[test]
fn config_has_default_ignores() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    let config_path = dir.path().join(VAULT_DIR).join(CONFIG_FILE);
    let contents = fs::read_to_string(config_path).expect("read config");
    assert!(contents.contains(".vault/**"));
    assert!(contents.contains("**/*.swp"));
    assert!(contents.contains("**/*.pdf"));
}

#[test]
fn partial_vault_reports_stray_files() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    let vault_dir = dir.path().join(VAULT_DIR);
    fs::create_dir_all(&vault_dir).expect("mkdir");
    fs::write(vault_dir.join(vault::paths::README_FILE), b"partial").expect("readme");

    common::vault_bin()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicates::str::contains("README"));
}

#[test]
fn sqlite_schema_matches_spec() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    let db_path = dir.path().join(VAULT_DIR).join(META_DB);
    assert_schema(&db_path);
}

fn assert_schema(db_path: &Path) {
    let conn = Connection::open(db_path).expect("open meta.db");

    let mut tables = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("prepare tables query");
    let names: Vec<String> = tables
        .query_map([], |row| row.get(0))
        .expect("query tables")
        .map(|r| r.expect("row"))
        .collect();

    assert!(names.contains(&"snapshots".to_string()));
    assert!(names.contains(&"file_events".to_string()));

    assert_index_exists(&conn, IDX_FILE_EVENTS_PATH_TIME);
    assert_index_exists(&conn, IDX_SNAPSHOTS_CREATED_AT);
}

fn assert_index_exists(conn: &Connection, name: &str) {
    let index_count: i64 = conn
        .query_row(COUNT_INDEX_BY_NAME, [name], |row| row.get(0))
        .expect("query index");
    assert_eq!(index_count, 1, "expected index {name}");
}
