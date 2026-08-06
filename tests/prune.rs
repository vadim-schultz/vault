//! Integration tests for `vault prune`.

mod common;

use std::fs;

use tempfile::TempDir;
use vault::registry::VaultRegistry;

#[test]
fn prune_removes_missing_vault_and_keeps_present_one() {
    let _env = common::VaultEnv::new();
    let present = TempDir::new().expect("tempdir present");
    let missing = TempDir::new().expect("tempdir missing");
    common::init_in(present.path());
    common::init_in(missing.path());

    let missing_root = missing.path().canonicalize().expect("canon");
    fs::remove_dir_all(missing.path()).expect("remove missing vault dir");

    common::vault_bin()
        .arg("prune")
        .assert()
        .success()
        .stdout(predicates::str::contains("Removed 1 missing vault(s):"))
        .stdout(predicates::str::contains(
            missing_root.display().to_string(),
        ));

    let registry = VaultRegistry::load().expect("load");
    assert_eq!(registry.vault.len(), 1);
    assert_eq!(
        registry.vault[0].root,
        present.path().canonicalize().expect("canon")
    );
}

#[test]
fn prune_reports_nothing_to_do_when_all_roots_present() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    common::init_in(dir.path());

    common::vault_bin()
        .arg("prune")
        .assert()
        .success()
        .stdout(predicates::str::contains("No missing vaults to prune."));

    let registry = VaultRegistry::load().expect("load");
    assert_eq!(registry.vault.len(), 1);
}

#[test]
fn second_prune_is_idempotent() {
    let _env = common::VaultEnv::new();
    let missing = TempDir::new().expect("tempdir missing");
    common::init_in(missing.path());
    fs::remove_dir_all(missing.path()).expect("remove missing vault dir");

    common::vault_bin()
        .arg("prune")
        .assert()
        .success()
        .stdout(predicates::str::contains("Removed 1 missing vault(s):"));

    common::vault_bin()
        .arg("prune")
        .assert()
        .success()
        .stdout(predicates::str::contains("No missing vaults to prune."));
}
