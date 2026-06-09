//! Windows shim embedding (Task 9.9) + cross-emission e2e via the binary (9.10).
//! Script emission is runtime-dispatched, so `--os windows` drives the Windows
//! activator on the linux lane.
#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use cuvm_cli::shims;
use predicates::str::contains;

// ---- Task 9.9: embedded shim content -----------------------------------------

#[test]
fn powershell_shim_evals_activation_verbs_and_passes_through() {
    let s = shims::windows_powershell();
    // Activation verbs go through Invoke-Expression of the printed env script.
    assert!(s.contains("Invoke-Expression"));
    assert!(s.contains("--shell powershell"));
    assert!(s.contains("'use','env','shell','default'"));
    // Non-activation commands pass straight through.
    assert!(s.contains("& cuvm.exe @args"));
}

#[test]
fn cmd_shim_uses_temp_bat_call_del() {
    let s = shims::windows_cmd();
    assert!(s.contains("--shell cmd --out"));
    assert!(s.to_lowercase().contains("call "));
    assert!(s.to_lowercase().contains("del "));
    assert!(s.contains("%TEMP%"));
}

// ---- Task 9.10: cross-emission via the binary --------------------------------

fn make_fake_toolkit(installs: &TempDir, ver: &str) -> std::path::PathBuf {
    installs
        .child(format!("cuda-{ver}/bin/nvcc"))
        .touch()
        .unwrap();
    installs
        .child(format!("cuda-{ver}/bin/nvcc.profile"))
        .touch()
        .unwrap();
    installs
        .child(format!("cuda-{ver}/lib64/libcudart.so"))
        .touch()
        .unwrap();
    installs.path().join(format!("cuda-{ver}"))
}

#[test]
fn env_powershell_emits_cuda_path() {
    let home = TempDir::new().unwrap();
    let installs = TempDir::new().unwrap();
    let p = make_fake_toolkit(&installs, "12.4");
    Command::cargo_bin("cuvm")
        .unwrap()
        .env("CUVM_HOME", home.path())
        .args(["adopt", p.to_str().unwrap()])
        .assert()
        .success();

    // The Windows activator is selected via --os regardless of the linux host.
    Command::cargo_bin("cuvm")
        .unwrap()
        .env("CUVM_HOME", home.path())
        .args(["env", "12.4", "--shell", "powershell", "--os", "windows"])
        .assert()
        .success()
        .stdout(contains("$env:CUDA_PATH"))
        .stdout(contains("$env:CUVM_INJECTED"))
        .stdout(contains("-split ';'"));
}

#[test]
fn hook_powershell_emits_chained_prompt() {
    let home = TempDir::new().unwrap();
    Command::cargo_bin("cuvm")
        .unwrap()
        .env("CUVM_HOME", home.path())
        .args(["hook", "--shell", "powershell", "--os", "windows"])
        .assert()
        .success()
        .stdout(contains("function global:prompt"))
        .stdout(contains("__cuvm_prev_prompt"));
}
