//! Integration tests for the singleton watcher.

mod common;

use std::fs;
use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::watch;
use vault::config::VaultConfig;
use vault::domain::RelPath;
use vault::paths::{META_DB, VAULT_DIR};
use vault::registry::VaultRegistry;

fn set_fast_debounce(worktree: &std::path::Path) {
    let config_path = worktree.join(VAULT_DIR).join(vault::paths::CONFIG_FILE);
    let mut config = VaultConfig::load(&config_path).expect("load config");
    config.watcher.debounce_ms = 100;
    config.write_to(&config_path).expect("write config");
}

fn snapshot_count(worktree: &std::path::Path) -> i64 {
    let db = worktree.join(VAULT_DIR).join(META_DB);
    vault::storage::sqlite::snapshot_count(&db).expect("count")
}

async fn start_watcher() -> watch::Sender<bool> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        vault::watcher::run(shutdown_rx).await.expect("watcher run");
    });
    shutdown_tx
}

#[test]
fn worker_commits_file_change() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("dir");
    fs::write(dir.path().join("a.md"), b"a").expect("write");
    common::init_in(dir.path());
    let baseline = snapshot_count(dir.path());

    let registry = VaultRegistry::load().expect("registry");
    let router = vault::watcher::Router::from_registry(&registry).expect("router");
    let batches = router.route(vec![dir.path().join("a.md")]);
    let vault = batches[0].0.clone();

    fs::write(dir.path().join("a.md"), b"a2").expect("write");
    vault::watcher::worker::commit_batch(&vault, &[RelPath::parse("a.md")]).expect("commit");

    assert!(snapshot_count(dir.path()) > baseline);
}

#[tokio::test]
async fn edit_triggers_snapshot_in_correct_vault() {
    let _env = common::VaultEnv::new();
    let dir_a = TempDir::new().expect("dir a");
    let dir_b = TempDir::new().expect("dir b");
    fs::write(dir_a.path().join("a.md"), b"a").expect("write");
    fs::write(dir_b.path().join("b.md"), b"b").expect("write");
    common::init_in(dir_a.path());
    common::init_in(dir_b.path());
    set_fast_debounce(dir_a.path());
    set_fast_debounce(dir_b.path());

    let baseline_a = snapshot_count(dir_a.path());
    let baseline_b = snapshot_count(dir_b.path());

    let shutdown_tx = start_watcher().await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    fs::write(dir_a.path().join("a.md"), b"a2").expect("edit a");
    common::wait_for_async(Duration::from_secs(5), || {
        snapshot_count(dir_a.path()) > baseline_a
    })
    .await;
    assert_eq!(snapshot_count(dir_b.path()), baseline_b);

    fs::write(dir_b.path().join("b.md"), b"b2").expect("edit b");
    common::wait_for_async(Duration::from_secs(5), || {
        snapshot_count(dir_b.path()) > baseline_b
    })
    .await;

    let _ = shutdown_tx.send(true);
}

#[tokio::test]
async fn ignored_swap_file_does_not_snapshot() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("dir");
    fs::write(dir.path().join("notes.md"), b"n").expect("write");
    common::init_in(dir.path());
    set_fast_debounce(dir.path());
    let baseline = snapshot_count(dir.path());

    let shutdown_tx = start_watcher().await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    fs::write(dir.path().join("notes.md.swp"), b"swap").expect("swp");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(snapshot_count(dir.path()), baseline);

    let _ = shutdown_tx.send(true);
}

#[tokio::test]
async fn delete_records_delete_event() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("dir");
    let file = dir.path().join("gone.md");
    fs::write(&file, b"x").expect("write");
    common::init_in(dir.path());
    set_fast_debounce(dir.path());

    let shutdown_tx = start_watcher().await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    fs::remove_file(&file).expect("remove");
    common::wait_for_async(Duration::from_secs(5), || {
        let db = dir.path().join(VAULT_DIR).join(META_DB);
        vault::storage::sqlite::latest_event_type(&db, "gone.md")
            .ok()
            .flatten()
            .as_deref()
            == Some("delete")
    })
    .await;

    let _ = shutdown_tx.send(true);
}

#[tokio::test]
async fn hot_reload_picks_up_new_registry_entry() {
    let _env = common::VaultEnv::new();
    let dir_a = TempDir::new().expect("dir a");
    let dir_b = TempDir::new().expect("dir b");
    fs::write(dir_a.path().join("a.md"), b"a").expect("write");
    common::init_in(dir_a.path());
    set_fast_debounce(dir_a.path());

    let shutdown_tx = start_watcher().await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    fs::write(dir_b.path().join("b.md"), b"b").expect("write");
    common::init_in(dir_b.path());
    set_fast_debounce(dir_b.path());
    tokio::time::sleep(Duration::from_millis(600)).await;

    let baseline_b = snapshot_count(dir_b.path());
    fs::write(dir_b.path().join("b.md"), b"b2").expect("edit");
    common::wait_for_async(Duration::from_secs(8), || {
        snapshot_count(dir_b.path()) > baseline_b
    })
    .await;

    let _ = shutdown_tx.send(true);
}

#[test]
fn registry_lists_empty_before_init() {
    let _env = common::VaultEnv::new();
    let registry = VaultRegistry::load().expect("load");
    assert!(registry.vault.is_empty());
}
