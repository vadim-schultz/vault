//! Integration tests for the singleton daemon.

mod common;

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::watch;
use vault::daemon::DaemonGuard;
use vault::error::VaultError;

#[test]
fn second_acquire_in_process_fails() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    let _first = DaemonGuard::acquire().expect("first lock");
    match DaemonGuard::acquire() {
        Err(VaultError::DaemonAlreadyRunning { .. }) => {}
        Ok(_) => panic!("expected already running"),
        Err(other) => panic!("expected already running, got {other}"),
    }
}

#[tokio::test]
async fn watcher_shutdown_exits_promptly() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    let _guard = DaemonGuard::acquire().expect("lock");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let handle = tokio::spawn(async move { vault::watcher::run(shutdown_rx).await });

    shutdown_tx.send(true).expect("shutdown");
    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("timeout")
        .expect("join");
    assert!(result.is_ok());
}

#[tokio::test]
async fn lock_released_after_guard_dropped() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    {
        let _guard = DaemonGuard::acquire().expect("first lock");
    }
    let _second = DaemonGuard::acquire().expect("second lock after drop");
}
