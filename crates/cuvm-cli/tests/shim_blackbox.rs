//! Black-box shell integration tests for the cuvm shims.
//!
//! Protocol proved: under `bash --norc` (and `zsh -f` when zsh is available),
//! sourcing the shim + `cd` into a `.cuda-version` dir activates the pinned
//! toolkit, re-activation does not duplicate PATH, and leaving reverts to
//! the `default` alias.

use assert_cmd::cargo::cargo_bin;
use assert_fs::prelude::*;
use std::collections::HashMap;
use std::process::Command;

/// Seed `CUVM_HOME` with two adopted bundles + a `default` alias → `12.6.0`.
fn seed_home() -> assert_fs::TempDir {
    let home = assert_fs::TempDir::new().unwrap();
    for v in ["12.4.1", "12.6.0"] {
        home.child(format!("versions/{v}/bin"))
            .create_dir_all()
            .unwrap();
        home.child(format!("versions/{v}/lib64"))
            .create_dir_all()
            .unwrap();
    }
    home.child("manifest.json")
        .write_str(
            r#"{
"schema_version": 1,
"bundles": [
  {"version":"12.4.1","source":"adopted","path":"versions/12.4.1","cudnn":null,
   "components":["cuda_nvcc"],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"},
  {"version":"12.6.0","source":"adopted","path":"versions/12.6.0","cudnn":null,
   "components":["cuda_nvcc"],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}
],
"aliases": {"default":"12.6.0"},
"pins": {},
"last_driver": null
}"#,
        )
        .unwrap();
    home
}

fn parse_lines(stdout: &str) -> HashMap<String, String> {
    stdout
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Run a shell driver under a clean shell; return parsed key=value output.
fn run_driver(shell: &str, clean_flag: &str, driver: &str, shim: &str) -> HashMap<String, String> {
    let home = seed_home();
    let pinned = assert_fs::TempDir::new().unwrap();
    pinned.child(".cuda-version").write_str("12.4.1\n").unwrap();
    let unpinned = assert_fs::TempDir::new().unwrap();

    let bin = cargo_bin("cuvm");
    let bin_dir = bin.parent().unwrap();

    let out = Command::new(shell)
        .arg(clean_flag) // bash: --norc ; zsh: -f
        .arg(driver)
        .arg(bin_dir)
        .arg(home.path())
        .arg(shim)
        .arg(pinned.path())
        .arg(unpinned.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{shell} driver failed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    parse_lines(&String::from_utf8(out.stdout).unwrap())
}

fn manifest_dir() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

#[test]
fn bash_shim_activates_dedups_and_reverts() {
    let driver = format!("{}/tests/fixtures/run_shim_bash.sh", manifest_dir());
    let shim = format!("{}/../../shims/cuvm.sh", manifest_dir());
    let m = run_driver("bash", "--norc", &driver, &shim);

    assert!(
        m["CUDA_HOME_AFTER_PIN"].ends_with("versions/12.4.1"),
        "pinned dir must activate 12.4.1, got {}",
        m["CUDA_HOME_AFTER_PIN"]
    );
    assert_eq!(m["CURRENT_AFTER_PIN"], "12.4.1");
    assert_eq!(
        m["PATH_BIN_COUNT"], "1",
        "no PATH duplication on re-activation"
    );
    assert_eq!(
        m["CURRENT_AFTER_LEAVE"], "12.6.0",
        "leaving pinned dir reverts to default"
    );
}

/// zsh test — skipped gracefully when `zsh` is not on the PATH.
#[test]
fn zsh_shim_activates_dedups_and_reverts() {
    // Graceful skip if zsh is not installed.
    if std::process::Command::new("zsh")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: zsh not found on PATH");
        return;
    }

    let driver = format!("{}/tests/fixtures/run_shim_zsh.sh", manifest_dir());
    let shim = format!("{}/../../shims/cuvm.zsh", manifest_dir());
    let m = run_driver("zsh", "-f", &driver, &shim);

    assert!(m["CUDA_HOME_AFTER_PIN"].ends_with("versions/12.4.1"));
    assert_eq!(m["CURRENT_AFTER_PIN"], "12.4.1");
    assert_eq!(m["PATH_BIN_COUNT"], "1");
    assert_eq!(m["CURRENT_AFTER_LEAVE"], "12.6.0");
}
