//! Integration tests for CLI version output.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_prints_package_version() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("vault")?
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("vault 0.1.0"));

    Ok(())
}
