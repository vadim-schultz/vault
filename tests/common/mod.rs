//! Shared helpers for integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use tempfile::TempDir;
use vault::paths::{CONFIG_FILE, GIT_DIR, META_DB, README_FILE, VAULT_DIR};

static STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Isolated global state for one integration test.
pub struct VaultEnv {
    _state: TempDir,
    state_path: PathBuf,
    _env_lock: std::sync::MutexGuard<'static, ()>,
}

impl VaultEnv {
    /// Create a temp state dir and set `VAULT_STATE_DIR` + `VAULT_NO_SERVICE`.
    pub fn new() -> Self {
        let env_lock = STATE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let state = TempDir::new().expect("temp state dir");
        let state_path = state.path().to_path_buf();
        std::env::set_var(vault::paths::STATE_DIR_ENV, &state_path);
        std::env::set_var(vault::paths::NO_SERVICE_ENV, "1");
        Self {
            _state: state,
            state_path,
            _env_lock: env_lock,
        }
    }

    /// Return the state directory path.
    #[must_use]
    pub fn state_path(&self) -> &Path {
        &self.state_path
    }
}

/// Return a `vault` binary command for integration tests.
pub fn vault_bin() -> Command {
    Command::cargo_bin("vault").expect("vault binary")
}

/// Run `vault init` in `dir` and assert success.
pub fn init_in(dir: &Path) -> assert_cmd::assert::Assert {
    vault_bin()
        .env(vault::paths::NO_SERVICE_ENV, "1")
        .current_dir(dir)
        .arg("init")
        .assert()
        .success()
}

fn missing(name: &str) -> String {
    format!("missing {name}")
}

/// Assert that `.vault/` contains all expected init artifacts.
pub fn assert_vault_layout(worktree: &Path) {
    let vault_dir = worktree.join(VAULT_DIR);
    assert!(vault_dir.is_dir(), "{}", missing(VAULT_DIR));
    assert!(
        vault_dir.join(README_FILE).is_file(),
        "{}",
        missing(README_FILE)
    );
    assert!(
        vault_dir.join(CONFIG_FILE).is_file(),
        "{}",
        missing(CONFIG_FILE)
    );
    assert!(vault_dir.join(META_DB).is_file(), "{}", missing(META_DB));
    assert!(vault_dir.join(GIT_DIR).is_dir(), "{}", missing(GIT_DIR));
}

/// Assert that `vault init` did not create a root `.git` entry.
pub fn assert_no_root_git(worktree: &Path) {
    assert!(
        !worktree.join(GIT_DIR).exists(),
        "vault init must not create root {}",
        GIT_DIR
    );
}

/// Poll `predicate` until it returns true or `timeout` elapses.
pub fn wait_for(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out after {:?}", timeout);
}

/// Async variant for `#[tokio::test]` — does not block the runtime worker thread.
pub async fn wait_for_async(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return;
        }
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out after {:?}", timeout);
}

/// Write `content` to `rel` (relative to `worktree`) and commit it via the real
/// snapshot pipeline (bypassing the watcher's debounce).
pub fn write_and_commit(worktree: &Path, rel: &str, content: &[u8]) {
    fs::write(worktree.join(rel), content).expect("write");
    let layout = vault::domain::VaultLayout::from_worktree(worktree.to_path_buf());
    vault::watcher::worker::commit_batch(&layout, &[vault::domain::RelPath::parse(rel)])
        .expect("commit");
}

/// Overwrite the most recently inserted snapshot's `created_at`, for deterministic
/// `--at` fixtures. Must be called immediately after `write_and_commit`.
pub fn backdate_last_snapshot(worktree: &Path, created_at: &str) {
    use rusqlite::params;

    let db_path = worktree.join(VAULT_DIR).join(META_DB);
    let conn = rusqlite::Connection::open(db_path).expect("open meta.db");
    conn.execute(
        "UPDATE snapshots SET created_at = ?1 WHERE id = (SELECT MAX(id) FROM snapshots)",
        params![created_at],
    )
    .expect("backdate");
}

/// Convenience: write + commit + backdate in one call.
pub fn snapshot_at(worktree: &Path, rel: &str, content: &[u8], created_at: &str) {
    write_and_commit(worktree, rel, content);
    backdate_last_snapshot(worktree, created_at);
}
