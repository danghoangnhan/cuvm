//! Black-box e2e for `cuvm adopt --scan` and `cuvm ls` over a fixture tree.
//! Runs on the linux/wsl CI lane. No real CUDA — empty files mimic the layout.
#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use predicates::prelude::*;

fn fixture(scan: &TempDir, ver: &str) {
    scan.child(format!("cuda-{ver}/bin/nvcc")).touch().unwrap();
    scan.child(format!("cuda-{ver}/bin/nvcc.profile"))
        .touch()
        .unwrap();
    scan.child(format!("cuda-{ver}/lib64/libcudart.so"))
        .touch()
        .unwrap();
}

fn cuvm(home: &TempDir, scan: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cuvm").unwrap();
    cmd.env("CUVM_HOME", home.path())
        .env("CUVM_SCAN_ROOT", scan.path());
    cmd
}

#[test]
fn adopt_scan_records_both_versions_and_ls_shows_them() {
    let home = TempDir::new().unwrap();
    let scan = TempDir::new().unwrap();
    fixture(&scan, "12.4");
    fixture(&scan, "11.8");

    cuvm(&home, &scan)
        .args(["adopt", "--scan"])
        .assert()
        .success()
        .stdout(predicate::str::contains("12.4").and(predicate::str::contains("11.8")));

    // Manifest now persists both as adopted, in place.
    cuvm(&home, &scan)
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("12.4").and(predicate::str::contains("11.8")));

    let manifest = std::fs::read_to_string(home.path().join("manifest.json")).unwrap();
    assert!(
        manifest.contains("\"adopted\""),
        "source must be recorded adopted"
    );
    assert!(manifest.contains(&scan.path().join("cuda-12.4").display().to_string()));
}

#[test]
fn deregister_removes_from_manifest_but_keeps_external_dir() {
    let home = TempDir::new().unwrap();
    let scan = TempDir::new().unwrap();
    fixture(&scan, "12.4");

    cuvm(&home, &scan)
        .args(["adopt", "--scan"])
        .assert()
        .success();

    // uninstall an adopted install => DE-REGISTER only (ADR-005).
    cuvm(&home, &scan)
        .args(["uninstall", "12.4"])
        .assert()
        .success();

    // Gone from the manifest...
    cuvm(&home, &scan)
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("12.4").not());

    // ...but the external dir + its files are STILL THERE (never deleted).
    scan.child("cuda-12.4/bin/nvcc")
        .assert(predicate::path::is_file());
    scan.child("cuda-12.4/bin/nvcc.profile")
        .assert(predicate::path::is_file());
    scan.child("cuda-12.4/lib64/libcudart.so")
        .assert(predicate::path::is_file());
}
