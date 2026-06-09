//! Black-box M1 end-to-end flow on the linux/wsl lane: adopt two fixture
//! toolkits in place, then exercise the user-visible verbs — ls, default,
//! current, use, which, pin, alias/unalias, default --link, and doctor.
//! No real CUDA: empty files mimic the toolkit layout (M1 never executes nvcc).
#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use predicates::prelude::*;

/// Lay down a minimal adoptable `cuda-<ver>` tree (bin/nvcc + lib64) and return
/// its absolute path.
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

/// A `cuvm` invocation rooted at an isolated `CUVM_HOME`.
fn cuvm(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cuvm").unwrap();
    cmd.env("CUVM_HOME", home.path());
    cmd
}

/// Adopt a fixture toolkit in place by path.
fn adopt(home: &TempDir, path: &std::path::Path) {
    cuvm(home)
        .args(["adopt", path.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn e2e_adopt_two_then_default_use_current() {
    let home = TempDir::new().unwrap();
    let installs = TempDir::new().unwrap();
    let p1 = make_fake_toolkit(&installs, "12.4.1");
    let p2 = make_fake_toolkit(&installs, "12.6.0");

    adopt(&home, &p1);
    adopt(&home, &p2);

    // ls shows both adopted toolkits.
    cuvm(&home)
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("12.4.1").and(predicate::str::contains("12.6.0")));

    // default -> 12.6.0 writes the `default` alias.
    cuvm(&home).args(["default", "12.6.0"]).assert().success();

    // ls now marks the default with `*`.
    cuvm(&home)
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("12.6.0 *"));

    // current with no breadcrumb resolves the default alias.
    cuvm(&home)
        .env_remove("CUVM_CURRENT")
        .arg("current")
        .assert()
        .success()
        .stdout(predicate::str::contains("12.6.0"));

    // use 12.4.1 emits an env script for bash referencing the 12.4.1 root.
    cuvm(&home)
        .args(["use", "12.4.1", "--shell", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("export CUVM_CURRENT=\"12.4.1\""))
        .stdout(predicate::str::contains("cuda-12.4.1"));
}

#[test]
fn which_prints_absolute_toolkit_root() {
    let home = TempDir::new().unwrap();
    let installs = TempDir::new().unwrap();
    let p = make_fake_toolkit(&installs, "12.4.1");
    adopt(&home, &p);

    cuvm(&home)
        .args(["which", "12.4.1"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("/"))
        .stdout(predicate::str::contains("cuda-12.4.1"));
}

#[test]
fn pin_writes_cuda_version_file_in_cwd() {
    let home = TempDir::new().unwrap();
    let installs = TempDir::new().unwrap();
    let workdir = TempDir::new().unwrap();
    let p = make_fake_toolkit(&installs, "12.6.0");
    adopt(&home, &p);

    // pin validates the spec resolves, then writes `.cuda-version` in cwd.
    cuvm(&home)
        .current_dir(workdir.path())
        .args(["pin", "12.6.0"])
        .assert()
        .success();

    workdir
        .child(".cuda-version")
        .assert(predicate::str::contains("12.6.0"));
}

#[test]
fn pin_rejects_unresolvable_spec() {
    let home = TempDir::new().unwrap();
    let workdir = TempDir::new().unwrap();
    // Nothing adopted: pinning a non-installed spec must fail and write nothing.
    cuvm(&home)
        .current_dir(workdir.path())
        .args(["pin", "99.9.9"])
        .assert()
        .failure();
    workdir
        .child(".cuda-version")
        .assert(predicate::path::missing());
}

#[test]
fn alias_then_which_then_unalias() {
    let home = TempDir::new().unwrap();
    let installs = TempDir::new().unwrap();
    let p = make_fake_toolkit(&installs, "12.6.0");
    adopt(&home, &p);

    // alias stable -> 12.6.0, then `which stable` resolves through the alias.
    cuvm(&home)
        .args(["alias", "stable", "12.6.0"])
        .assert()
        .success();
    cuvm(&home)
        .args(["which", "stable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cuda-12.6.0"));

    // unalias removes it; `which stable` then fails to resolve.
    cuvm(&home).args(["unalias", "stable"]).assert().success();
    cuvm(&home).args(["which", "stable"]).assert().failure();
}

#[test]
fn default_link_creates_current_pointer() {
    let home = TempDir::new().unwrap();
    let installs = TempDir::new().unwrap();
    let p = make_fake_toolkit(&installs, "12.6.0");
    adopt(&home, &p);

    cuvm(&home)
        .args(["default", "12.6.0", "--link"])
        .assert()
        .success();

    let pointer = home.path().join("current");
    let meta = std::fs::symlink_metadata(&pointer).expect("current pointer exists");
    assert!(meta.file_type().is_symlink(), "current must be a symlink");
    let target = std::fs::read_link(&pointer).unwrap();
    assert!(
        target.to_string_lossy().contains("versions/12.6.0"),
        "current -> {} should point at versions/12.6.0",
        target.display()
    );
}

#[test]
fn doctor_with_absent_driver_warns_build_only() {
    let home = TempDir::new().unwrap();
    // Empty PATH dir => `nvidia-smi` is not found => driver absent (graceful).
    let empty = TempDir::new().unwrap();
    cuvm(&home)
        .env("PATH", empty.path())
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("CUDA_HOME")
        .env_remove("CUVM_CURRENT")
        .arg("doctor")
        .assert()
        .code(1) // one Warn (driver absent), no Block => exit 1
        .stdout(predicate::str::contains("GPU driver not detected"));
}
