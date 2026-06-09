//! E2e tests for `cuvm env <spec> --shell bash`.

use assert_cmd::Command;
use assert_fs::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

/// Build a minimal `CUVM_HOME` with one adopted bundle so the `Resolver` finds it.
fn seed_home() -> assert_fs::TempDir {
    let home = assert_fs::TempDir::new().unwrap();
    // versions/12.4.1 tree (adopted-style, has lib64)
    home.child("versions/12.4.1/bin").create_dir_all().unwrap();
    home.child("versions/12.4.1/lib64")
        .create_dir_all()
        .unwrap();
    // manifest.json registering the bundle (schema per cuvm-store / WU-3).
    home.child("manifest.json")
        .write_str(
            r#"{
"schema_version": 1,
"bundles": [
  { "version": "12.4.1", "source": "adopted",
    "path": "versions/12.4.1", "cudnn": null,
    "components": ["cuda_nvcc","cuda_cudart"], "sha256": null,
    "installed_at": "2026-06-08T00:00:00Z" }
],
"aliases": {},
"pins": {},
"last_driver": null
}"#,
        )
        .unwrap();
    home
}

#[test]
fn env_exact_spec_emits_cuda_home() {
    let home = seed_home();
    Command::cargo_bin("cuvm")
        .unwrap()
        .env("CUVM_HOME", home.path())
        .args(["env", "12.4.1", "--shell", "bash"])
        .assert()
        .success()
        .stdout(contains("export CUDA_HOME=").and(contains("versions/12.4.1")))
        .stdout(contains("export CUVM_INJECTED="));
}

#[test]
fn env_default_on_empty_home_emits_deactivate() {
    let home = assert_fs::TempDir::new().unwrap();
    home.child("manifest.json")
        .write_str(r#"{"schema_version":1,"bundles":[],"aliases":{},"pins":{},"last_driver":null}"#)
        .unwrap();
    Command::cargo_bin("cuvm")
        .unwrap()
        .env("CUVM_HOME", home.path())
        .args(["env", "default", "--shell", "bash"])
        .assert()
        .success()
        // deactivate strips the breadcrumb and unsets CUVM_CURRENT
        .stdout(contains("unset CUVM_CURRENT").or(contains("CUVM_INJECTED")));
}
