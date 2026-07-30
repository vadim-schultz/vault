//! Shared helpers for integration tests.

use std::path::Path;

use assert_cmd::Command;
use vault::paths::{CONFIG_FILE, GIT_DIR, META_DB, README_FILE, VAULT_DIR};

/// Return a `vault` binary command for integration tests.
pub fn vault_bin() -> Command {
    Command::cargo_bin("vault").expect("vault binary")
}

/// Run `vault init` in `dir` and assert success.
pub fn init_in(dir: &Path) -> assert_cmd::assert::Assert {
    vault_bin().current_dir(dir).arg("init").assert().success()
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
