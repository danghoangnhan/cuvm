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

/// `cuvm --help` output is pinned by a golden snapshot so any change to the
/// command surface (subcommands added in later WUs) is a reviewable diff.
#[test]
fn help_output_matches_snapshot() {
    let output = Command::cargo_bin("cuvm")
        .expect("binary `cuvm` is built")
        .arg("--help")
        .output()
        .expect("run cuvm --help");
    assert!(output.status.success(), "cuvm --help should exit 0");
    let stdout = String::from_utf8(output.stdout).expect("help text is utf-8");
    insta::assert_snapshot!("help", stdout);
}
