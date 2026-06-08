use assert_cmd::Command;
use predicates::prelude::*;

/// `cuvm --version` prints `cuvm <semver>` and exits 0.
#[test]
fn version_flag_prints_name_and_version() {
    Command::cargo_bin("cuvm")
        .expect("binary `cuvm` is built")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("cuvm "));
}
