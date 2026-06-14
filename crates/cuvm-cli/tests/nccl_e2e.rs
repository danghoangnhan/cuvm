//! E2e tests for `cuvm nccl install` / `cuvm nccl ls` (M4 / WU-20b) against
//! FAKE NCCL fixtures served by httpmock — no real network, no GPU. The NCCL
//! redist ships no checksums, so the download path self-records the sha256.

#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary builds")
}

/// Seed a `CUVM_HOME` with one DOWNLOADED 12.4.1 toolkit (NCCL links into it).
fn seed_home() -> TempDir {
    let home = TempDir::new().unwrap();
    home.child("versions/12.4.1/lib").create_dir_all().unwrap();
    home.child("manifest.json")
        .write_str(
            r#"{"schema_version":1,"bundles":[
  {"version":"12.4.1","source":"downloaded","path":"versions/12.4.1","cudnn":null,
   "components":["cuda_nvcc"],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}
],"aliases":{},"pins":{},"last_driver":null}"#,
        )
        .unwrap();
    home
}

/// Build an NCCL `.txz` (tar.xz): wrapper `nccl_<ver>-<build>+cuda<cuda>_<arch>/`
/// with `lib/libnccl.so{,.2}` + `include/nccl.h`. Shells out to system `tar`
/// (the workspace ships no xz *encoder*, only the pure-Rust decoder). Returns
/// the archive path + bytes.
fn make_nccl_txz(dir: &Path, ver: &str, cuda: &str, arch: &str) -> std::path::PathBuf {
    use std::process::Command as Proc;
    let wrapper = format!("nccl_{ver}-1+cuda{cuda}_{arch}");
    let staging = dir.join(format!("stage-{wrapper}"));
    for (rel, body) in [
        ("lib/libnccl.so", "NCCL\n"),
        ("lib/libnccl.so.2", "NCCL2\n"),
        ("include/nccl.h", "// nccl\n"),
    ] {
        let p = staging.join(&wrapper).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let archive = dir.join(format!("{wrapper}.txz"));
    let status = Proc::new("tar")
        .arg("-cJf")
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg(&wrapper)
        .status()
        .expect("tar -cJf builds the nccl fixture");
    assert!(status.success());
    archive
}

/// Stand up the NCCL redist for `2.21.5/cuda12.4/x86_64`: index → version dir →
/// the `.txz` bytes (served verbatim; the client self-records its sha).
fn serve_nccl(server: &MockServer, fixtures: &Path) {
    let archive = make_nccl_txz(fixtures, "2.21.5", "12.4", "x86_64");
    let bytes = std::fs::read(&archive).unwrap();
    server.mock(|when, then| {
        when.method(GET).path("/nccl/");
        then.status(200).body(
            "<html><body><a href='v2.20.5/'>v2.20.5/</a><a href='v2.21.5/'>v2.21.5/</a></body></html>",
        );
    });
    server.mock(|when, then| {
        when.method(GET).path("/nccl/v2.21.5/");
        then.status(200).body(
            "<html><body><a href='nccl_2.21.5-1+cuda12.4_x86_64.txz'>nccl_2.21.5-1+cuda12.4_x86_64.txz</a></body></html>",
        );
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/nccl/v2.21.5/nccl_2.21.5-1+cuda12.4_x86_64.txz");
        then.status(200).body(bytes.clone());
    });
}

fn cuvm_with(home: &TempDir, nccl_url: &str) -> Command {
    let mut c = cuvm();
    c.env("CUVM_HOME", home.path())
        .env("CUVM_NCCL_REGISTRY_URL", nccl_url);
    c
}

#[test]
fn nccl_install_downloads_self_records_links_and_records_sidecar() {
    let home = seed_home();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_nccl(&server, fixtures.path());
    let nccl_url = format!("{}/nccl/", server.base_url());

    cuvm_with(&home, &nccl_url)
        .args(["nccl", "install", "2.21", "--for", "12.4.1"])
        .assert()
        .success()
        .stdout(contains("+ nccl 2.21.5 (cuda12)  ->  12.4.1"));

    // Full libnccl* set symlinked into the toolkit (loader + soname + header).
    let linked = home.child("versions/12.4.1/lib/libnccl.so");
    assert!(
        std::fs::symlink_metadata(linked.path())
            .unwrap()
            .file_type()
            .is_symlink(),
        "libnccl.so must be a symlink into the content store"
    );
    home.child("versions/12.4.1/lib/libnccl.so.2")
        .assert(predicates::path::exists());
    home.child("versions/12.4.1/include/nccl.h")
        .assert(predicates::path::exists());
    // The sidecar records the pairing; the store key is the self-recorded sha.
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.child("versions/12.4.1/.cuvm-nccl.json").path()).unwrap(),
    )
    .unwrap();
    assert_eq!(meta["version"], "2.21.5", "{meta}");
    assert_eq!(meta["cuda_major"], 12, "{meta}");
    let sha = meta["sha256"].as_str().unwrap();
    assert_eq!(sha.len(), 64, "self-recorded sha256 is 64 hex chars");
    home.child(format!("nccl/{sha}/lib/libnccl.so"))
        .assert(predicates::path::exists());
}

#[test]
fn nccl_ls_shows_the_pairing_then_reports_empty() {
    let home = seed_home();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_nccl(&server, fixtures.path());
    let nccl_url = format!("{}/nccl/", server.base_url());

    // Empty before any install.
    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["nccl", "ls"])
        .assert()
        .success()
        .stdout(contains("(no nccl payloads)"));

    cuvm_with(&home, &nccl_url)
        .args(["nccl", "install", "2.21.5", "--for", "12.4.1"])
        .assert()
        .success();

    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["nccl", "ls"])
        .assert()
        .success()
        .stdout(contains("2.21.5 (cuda12)").and(contains("->  12.4.1")));
}

#[test]
fn nccl_install_ingests_a_user_supplied_archive_without_network() {
    let home = seed_home();
    let fixtures = TempDir::new().unwrap();
    // The cuDNN/NCCL bases stay unroutable: a supplied archive needs no network.
    let archive = make_nccl_txz(fixtures.path(), "2.21.5", "12.4", "x86_64");

    cuvm_with(&home, "http://127.0.0.1:1/nccl/")
        .args([
            "nccl",
            "install",
            archive.to_str().unwrap(),
            "--for",
            "12.4.1",
        ])
        .assert()
        .success()
        .stdout(contains("+ nccl 2.21.5 (cuda12)  ->  12.4.1"));
    home.child("versions/12.4.1/lib/libnccl.so.2")
        .assert(predicates::path::exists());
}

#[test]
fn nccl_install_refuses_a_cuda_major_mismatch() {
    let home = seed_home();
    let fixtures = TempDir::new().unwrap();
    // A cuda11 NCCL archive against the cuda12 toolkit must be refused.
    let archive = make_nccl_txz(fixtures.path(), "2.18.3", "11.0", "x86_64");

    cuvm()
        .env("CUVM_HOME", home.path())
        .args([
            "nccl",
            "install",
            archive.to_str().unwrap(),
            "--for",
            "12.4.1",
        ])
        .assert()
        .failure()
        .stderr(contains("CUDA 11").and(contains("CUDA 12")));
    home.child("versions/12.4.1/.cuvm-nccl.json")
        .assert(predicates::path::missing());
}

#[test]
fn nccl_install_refuses_an_adopted_target() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    home.child("manifest.json")
        .write_str(
            r#"{"schema_version":1,"bundles":[
  {"version":"12.4","source":"adopted","path":"/usr/local/cuda-12.4","cudnn":null,
   "components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}
],"aliases":{},"pins":{},"last_driver":null}"#,
        )
        .unwrap();
    let archive = make_nccl_txz(fixtures.path(), "2.21.5", "12.4", "x86_64");

    cuvm()
        .env("CUVM_HOME", home.path())
        .args([
            "nccl",
            "install",
            archive.to_str().unwrap(),
            "--for",
            "12.4",
        ])
        .assert()
        .failure()
        .stderr(contains("adopted"));
}

#[test]
fn nccl_help_surfaces_in_the_command_tree() {
    cuvm()
        .args(["nccl", "--help"])
        .assert()
        .success()
        .stdout(contains("install").and(contains("ls")));
    cuvm()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("nccl"));
}
