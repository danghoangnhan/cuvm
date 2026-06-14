//! WU-19 — cross-cutting *full-lifecycle* integration harness.
//!
//! The per-command e2e suites (`m1_e2e`, `install_e2e`, `nccl_e2e`, …) each prove
//! one verb in isolation. This harness proves the **layers compose**: a single
//! continuous flow drives the documented user journey end to end on fake redist
//! fixtures served by `httpmock` — no real network, no GPU — exercising
//!
//!   install → ls → default → current → use → pin → cd-switch → cuDNN pairing →
//!   NCCL companion → doctor
//!
//! against the *built* `cuvm` binary. A final `#[ignore]`d test runs the same
//! journey against a REAL toolkit when one is available (gated by `CUVM_SMOKE`),
//! mirroring the real-`nvcc` smoke gate in `cuvm-platform/tests`.
//!
//! Unix-only: the install path asserts the Linux `lib64 -> lib` symlink and uses
//! a fake `nvidia-smi`, so the whole module is `#[cfg(unix)]`.
#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;

// ----------------------------------------------------------------------------
// Fixture builders (redist-shaped `.tar.xz` via the system `tar`; the workspace
// ships only a pure-Rust xz *decoder*, so encoding shells out — same contract as
// `install_e2e`).
// ----------------------------------------------------------------------------

/// Build a redist toolkit component `.tar.xz`: wrapper
/// `<comp>-linux-x86_64-<ver>-archive/` holding `bin/nvcc` + `lib/<lib>`.
/// Returns `(bytes, sha256-hex)`.
fn make_component_tarxz(dir: &Path, comp: &str, ver: &str, lib: &str) -> (Vec<u8>, String) {
    let wrapper = format!("{comp}-linux-x86_64-{ver}-archive");
    let staging = dir.join(format!("stage-{comp}-{ver}"));
    for (rel, body) in [
        ("bin/nvcc", "#!/bin/sh\n".to_string()),
        (lib, "ELFPLACEHOLDER\n".to_string()),
    ] {
        let p = staging.join(&wrapper).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    tarxz(dir, &staging, &wrapper)
}

/// Build a cuDNN redist `.tar.xz`: wrapper
/// `cudnn-linux-x86_64-<ver>_cuda<major>-archive/` with the loader + one engine
/// sub-lib + a header (the "full set" contract needs more than one lib).
fn make_cudnn_tarxz(dir: &Path, ver: &str, cuda_major: u32) -> (Vec<u8>, String) {
    let wrapper = format!("cudnn-linux-x86_64-{ver}_cuda{cuda_major}-archive");
    let staging = dir.join(format!("stage-cudnn-{ver}"));
    for (rel, body) in [
        ("lib/libcudnn.so", "CUDNN\n"),
        ("lib/libcudnn_ops.so", "CUDNNOPS\n"),
        ("include/cudnn.h", "// cudnn\n"),
    ] {
        let p = staging.join(&wrapper).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    tarxz(dir, &staging, &wrapper)
}

/// Build a user-suppliable NCCL `.txz`: a single wrapper dir holding the full
/// `libnccl*` set under `lib/`. Returns the archive path (NCCL is ingested from
/// a local file, so the caller hands the path to `cuvm nccl install`).
fn make_nccl_txz(dir: &Path, file_name: &str) -> std::path::PathBuf {
    let wrapper = "nccl-stage";
    let staging = dir.join("stage-nccl");
    for (rel, body) in [
        ("lib/libnccl.so", "NCCL\n"),
        ("lib/libnccl.so.2", "NCCL2\n"),
    ] {
        let p = staging.join(wrapper).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let archive = dir.join(file_name);
    run_tar(&archive, &staging, wrapper);
    archive
}

/// `tar -cJf` the single `member` dir under `cwd`; return `(bytes, sha256-hex)`.
fn tarxz(dir: &Path, cwd: &Path, member: &str) -> (Vec<u8>, String) {
    use sha2::{Digest, Sha256};
    let archive = dir.join(format!("{member}.tar.xz"));
    run_tar(&archive, cwd, member);
    let bytes = std::fs::read(&archive).unwrap();
    let sha = format!("{:x}", Sha256::digest(&bytes));
    (bytes, sha)
}

fn run_tar(archive: &Path, cwd: &Path, member: &str) {
    let status = std::process::Command::new("tar")
        .arg("-cJf")
        .arg(archive)
        .arg("-C")
        .arg(cwd)
        .arg(member)
        .status()
        .expect("invoke `tar -cJf` to build the fixture archive");
    assert!(status.success(), "tar must build the .tar.xz fixture");
}

// ----------------------------------------------------------------------------
// Mock redist servers.
// ----------------------------------------------------------------------------

/// Serve a CUDA 12.4.1 redist: index → manifest (nvcc + cudart) → tarballs.
/// The recommended set for 12.x filters to the keys actually present, so two
/// components are enough to land a real `bin/nvcc` + `lib/` toolkit on disk.
fn serve_redist(server: &MockServer, fixtures: &Path) {
    let (nvcc_bytes, nvcc_sha) =
        make_component_tarxz(fixtures, "cuda_nvcc", "12.4.131", "lib/stub.so");
    let (cudart_bytes, cudart_sha) =
        make_component_tarxz(fixtures, "cuda_cudart", "12.4.131", "lib/libcudart.so");
    let nvcc_rel = "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-12.4.131-archive.tar.xz";
    let cudart_rel = "cuda_cudart/linux-x86_64/cuda_cudart-linux-x86_64-12.4.131-archive.tar.xz";

    let index = r#"<html><body>
        <a href="redistrib_12.4.1.json">redistrib_12.4.1.json</a>
        </body></html>"#;
    let redistrib = format!(
        r#"{{
  "release_date": "2024-03-01",
  "cuda_nvcc": {{
    "name": "CUDA nvcc",
    "version": "12.4.131",
    "linux-x86_64": {{ "relative_path": "{nvcc_rel}", "sha256": "{nvcc_sha}", "size": "{nvcc_size}" }}
  }},
  "cuda_cudart": {{
    "name": "CUDA Runtime",
    "version": "12.4.131",
    "linux-x86_64": {{ "relative_path": "{cudart_rel}", "sha256": "{cudart_sha}", "size": "{cudart_size}" }}
  }}
}}"#,
        nvcc_size = nvcc_bytes.len(),
        cudart_size = cudart_bytes.len(),
    );

    server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(index);
    });
    server.mock(|when, then| {
        when.method(GET).path("/redist/redistrib_12.4.1.json");
        then.status(200).body(redistrib);
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/redist/{nvcc_rel}"));
        then.status(200).body(nvcc_bytes.clone());
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/redist/{cudart_rel}"));
        then.status(200).body(cudart_bytes.clone());
    });
}

/// Serve a cuDNN redist that lists ONLY 9.8.0 (so the matrix-default pick for
/// CUDA 12.x is unambiguous and the default pairing is deterministic).
fn serve_cudnn(server: &MockServer, fixtures: &Path) {
    let (bytes, sha) = make_cudnn_tarxz(fixtures, "9.8.0.87", 12);
    let rel = "cudnn/linux-x86_64/cudnn-linux-x86_64-9.8.0.87_cuda12-archive.tar.xz";
    let manifest = format!(
        r#"{{
  "release_label": "9.8.0",
  "cudnn": {{
    "license_path": "cudnn/LICENSE.txt",
    "version": "9.8.0.87",
    "linux-x86_64": {{ "cuda12": {{ "relative_path": "{rel}", "sha256": "{sha}", "size": "{size}" }} }}
  }}
}}"#,
        size = bytes.len()
    );
    server.mock(|when, then| {
        when.method(GET).path("/cudnn/");
        then.status(200).body(
            r#"<html><body><a href="redistrib_9.8.0.json">redistrib_9.8.0.json</a></body></html>"#,
        );
    });
    server.mock(|when, then| {
        when.method(GET).path("/cudnn/redistrib_9.8.0.json");
        then.status(200).body(manifest);
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/cudnn/{rel}"));
        then.status(200).body(bytes.clone());
    });
}

// ----------------------------------------------------------------------------
// CLI driver helpers.
// ----------------------------------------------------------------------------

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary builds")
}

/// `cuvm` wired for an offline run: fake `CUVM_HOME`, both registry overrides,
/// no driver (`nvidia-smi` forced absent → the compat gate proceeds, "build-only
/// OK"), and the post-install smoke test skipped (the fixtures ship a stub nvcc).
fn base(home: &TempDir, reg: &str, cudnn_reg: &str) -> Command {
    let mut c = cuvm();
    c.env("CUVM_HOME", home.path())
        .env("CUVM_REGISTRY_URL", reg)
        .env("CUVM_CUDNN_REGISTRY_URL", cudnn_reg)
        .env("CUVM_NVIDIA_SMI", "/nonexistent/nvidia-smi")
        .env("CUVM_SKIP_SMOKE", "1");
    c
}

// ----------------------------------------------------------------------------
// The headline lifecycle: install → ls → default → current → use → pin →
// cd-switch → cuDNN pairing → doctor, all on one toolkit, all offline.
// ----------------------------------------------------------------------------

#[test]
fn full_lifecycle_install_default_use_pin_cdswitch_cudnn_doctor() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist(&server, fixtures.path());
    serve_cudnn(&server, fixtures.path());
    let reg = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());
    let run = || base(&home, &reg, &cudnn_reg);

    // 1. install 12.4 with the default cuDNN pairing (EULA accepted up front).
    run()
        .args(["install", "12.4", "--accept-eula"])
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1"))
        .stdout(contains("cudnn"));

    // The toolkit, the lib64 -> lib symlink, and the linked cuDNN all landed.
    home.child("versions/12.4.1/bin/nvcc")
        .assert(predicates::path::exists());
    assert!(
        std::fs::symlink_metadata(home.child("versions/12.4.1/lib64").path())
            .unwrap()
            .file_type()
            .is_symlink(),
        "Linux install must create the lib64 -> lib symlink"
    );
    home.child("versions/12.4.1/lib/libcudnn.so")
        .assert(predicates::path::exists());

    // 2. ls renders the installed bundle.
    run()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("12.4.1"));

    // 3. default + current agree.
    run().args(["default", "12.4.1"]).assert().success();
    run()
        .env_remove("CUVM_CURRENT")
        .arg("current")
        .assert()
        .success()
        .stdout(contains("12.4.1"));

    // 4. `use` emits a bash activation script referencing the toolkit root.
    run()
        .args(["use", "12.4.1", "--shell", "bash"])
        .assert()
        .success()
        .stdout(contains("CUDA_HOME").and(contains("versions/12.4.1")));

    // 5. pin in a project dir, then cd-switch: `current` from that dir (with no
    //    breadcrumb) resolves the `.cuda-version` pin via the upward walk.
    let proj = TempDir::new().unwrap();
    run()
        .current_dir(proj.path())
        .args(["pin", "12.4.1"])
        .assert()
        .success();
    proj.child(".cuda-version")
        .assert(predicates::path::exists());
    run()
        .current_dir(proj.path())
        .env_remove("CUVM_CURRENT")
        .arg("current")
        .assert()
        .success()
        .stdout(contains("12.4.1"));

    // 6. doctor sees the recorded cuDNN pairing for the active toolkit. The
    //    driver is absent (DRIVER_ABSENT warn => exit 1); PATH is forced clean so
    //    hygiene stays OK and the pairing finding is the signal under test.
    run()
        .env("CUVM_CURRENT", "12.4.1")
        .env("PATH", "/usr/bin")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("CUDA_HOME")
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(contains("CUDNN_PAIRING"));
}

// ----------------------------------------------------------------------------
// Companion layering: a single bundle carries BOTH a paired cuDNN (from install)
// and a user-supplied NCCL, each linked into the same toolkit root.
// ----------------------------------------------------------------------------

#[test]
fn companion_layering_cudnn_and_nccl_share_one_toolkit() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist(&server, fixtures.path());
    serve_cudnn(&server, fixtures.path());
    let reg = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());
    let run = || base(&home, &reg, &cudnn_reg);

    run()
        .args(["install", "12.4", "--accept-eula"])
        .assert()
        .success();

    // NCCL is ingested from a local archive (no network, no EULA — BSD): build a
    // standard-named `.txz` for CUDA 12 and pair it with the installed toolkit.
    let nccl = make_nccl_txz(fixtures.path(), "nccl_2.21.5-1+cuda12.4_x86_64.txz");
    run()
        .args(["nccl", "install", nccl.to_str().unwrap(), "--for", "12.4.1"])
        .assert()
        .success()
        .stdout(contains("nccl"));

    // Both companions are linked under the one toolkit root.
    home.child("versions/12.4.1/lib/libcudnn.so")
        .assert(predicates::path::exists());
    home.child("versions/12.4.1/lib/libnccl.so")
        .assert(predicates::path::exists());

    // Both companion listings report the pairing against the same handle.
    run()
        .args(["cudnn", "ls"])
        .assert()
        .success()
        .stdout(contains("12.4.1"));
    run()
        .args(["nccl", "ls"])
        .assert()
        .success()
        .stdout(contains("12.4.1"));
}

// ----------------------------------------------------------------------------
// Adopt + download coexistence: an in-place adopted toolkit and a downloaded one
// live side by side in `ls`, and each resolves to its own root.
// ----------------------------------------------------------------------------

#[test]
fn adopted_and_downloaded_toolkits_coexist() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist(&server, fixtures.path());
    let reg = format!("{}/redist/", server.base_url());
    let cudnn_reg = "http://127.0.0.1:1/cudnn/"; // never hit: install uses --no-cudnn
    let run = || base(&home, &reg, cudnn_reg);

    // Adopt an external 12.2.0 toolkit in place (never copied — ADR-005). adopt
    // derives the version from the `cuda-<ver>` dir name, so mirror the m1_e2e
    // fixture shape exactly: bin/nvcc(+ .profile) + lib64/libcudart.so.
    let installs = TempDir::new().unwrap();
    for rel in [
        "cuda-12.2.0/bin/nvcc",
        "cuda-12.2.0/bin/nvcc.profile",
        "cuda-12.2.0/lib64/libcudart.so",
    ] {
        installs.child(rel).touch().unwrap();
    }
    let external = installs.path().join("cuda-12.2.0");
    run()
        .args(["adopt", external.to_str().unwrap()])
        .assert()
        .success();

    // Download 12.4.1 alongside it.
    run()
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1"));

    // `ls` shows both lines.
    run()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("12.2.0").and(contains("12.4.1")));

    // `which` resolves each to its own root: the adopted one stays external; the
    // downloaded one lives under CUVM_HOME/versions.
    run()
        .args(["which", "12.2.0"])
        .assert()
        .success()
        .stdout(contains("cuda-12.2.0"));
    run()
        .args(["which", "12.4.1"])
        .assert()
        .success()
        .stdout(contains("versions/12.4.1"));
}

// ----------------------------------------------------------------------------
// Real-toolkit lifecycle — opt-in (a real toolkit + host gcc are required).
// Run with: CUVM_SMOKE=1 CUVM_REAL_ROOT=/usr/local/cuda-12.x \
//           cargo test -p cuvm-cli --test lifecycle_e2e -- --ignored
// ----------------------------------------------------------------------------

#[test]
#[ignore = "requires a real toolkit; set CUVM_SMOKE=1 + CUVM_REAL_ROOT to run"]
fn real_toolkit_adopt_use_doctor() {
    assert_eq!(
        std::env::var("CUVM_SMOKE").as_deref(),
        Ok("1"),
        "guarded by CUVM_SMOKE=1"
    );
    let root = std::env::var("CUVM_REAL_ROOT").expect("set CUVM_REAL_ROOT to a real toolkit");
    let home = TempDir::new().unwrap();
    let run = || {
        let mut c = cuvm();
        c.env("CUVM_HOME", home.path());
        c
    };

    // Adopt the real install, make it the default, and confirm `use` emits an
    // activation script pointing at the real root — then doctor runs clean of
    // blocks (it may warn on driver ceiling, which is environment-dependent).
    run().args(["adopt", &root]).assert().success();
    let handle = run().arg("current").assert().success();
    let handle = String::from_utf8_lossy(&handle.get_output().stdout)
        .trim()
        .to_string();
    run()
        .args(["use", &handle, "--shell", "bash"])
        .assert()
        .success()
        .stdout(contains(&root));
    // doctor must not BLOCK (exit 2); warn (1) or ok (0) are both acceptable.
    let code = run().arg("doctor").assert().get_output().status.code();
    assert_ne!(code, Some(2), "real-toolkit doctor must not block");
}
