//! Integration tests for the singleton daemon.

mod common;

use tempfile::TempDir;
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
