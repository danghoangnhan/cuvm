//! End-to-end tests for `cuvm self update` against a mocked GitHub release
//! (httpmock — no real network). Unix-only: the fake release payload is a shell
//! script standing in for the cuvm binary, and CI runs its test lane on Linux.
//!
//! `CUVM_SELF_UPDATE_TARGET` points the swap at a throwaway file so the full
//! download → verify → extract → smoke-test → swap pipeline runs without ever
//! clobbering the cargo-built test binary.
#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use sha2::{Digest, Sha256};
use std::path::Path;

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary builds")
}

/// This host's release-asset platform tag (must match the binary's own mapping).
fn asset_name() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux-amd64",
        "aarch64" => "linux-arm64",
        other => panic!("test host arch {other} has no cuvm release asset"),
    }
}

/// Package `entries` (`(relpath, bytes)`, relative to the `cuvm-<ver>-<asset>`
/// stage dir) into a real gzip `.tar.gz` and return (archive bytes, sha256-hex).
/// Shells out to system `tar -czf` (the workspace ships only the pure-Rust gzip
/// *decoder*).
fn make_targz(dir: &Path, tag: &str, ver: &str, entries: &[(&str, &str)]) -> (Vec<u8>, String) {
    use std::fmt::Write;
    use std::process::Command as Proc;
    let stage = format!("cuvm-{ver}-{}", asset_name());
    let staging = dir.join(format!("stage-{tag}-{stage}"));
    for (rel, body) in entries {
        let p = staging.join(&stage).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let archive = dir.join(format!("{tag}-{stage}.tar.gz"));
    let status = Proc::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg(&stage)
        .status()
        .expect("invoke tar -czf to build fixture");
    assert!(status.success(), "tar must build the .tar.gz fixture");
    let bytes = std::fs::read(&archive).unwrap();
    let digest = Sha256::digest(&bytes);
    let mut sha = String::with_capacity(64);
    for b in &digest {
        write!(&mut sha, "{b:02x}").unwrap();
    }
    (bytes, sha)
}

/// A normal release archive: a runnable `cuvm` script printing `cuvm <ver>` (or
/// whatever `script` is) plus a `shims/cuvm.sh`.
fn make_release(dir: &Path, tag: &str, ver: &str, script: &str) -> (Vec<u8>, String) {
    make_targz(
        dir,
        tag,
        ver,
        &[("cuvm", script), ("shims/cuvm.sh", "# fake shim\n")],
    )
}

/// Stand up a mock release: `releases/latest` → `v<ver>`, a `SHA256SUMS` line
/// carrying `sums_sha` for the archive, and the archive bytes themselves.
fn mock_release(ver: &str, bytes: Vec<u8>, sums_sha: &str) -> MockServer {
    let archive = format!("cuvm-{ver}-{}.tar.gz", asset_name());
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/releases/latest");
        then.status(200)
            .body(format!("{{\"tag_name\":\"v{ver}\"}}"));
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/v{ver}/SHA256SUMS"));
        then.status(200).body(format!("{sums_sha}  {archive}\n"));
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/v{ver}/{archive}"));
        then.status(200).body(bytes);
    });
    server
}

/// Run `cuvm self update <extra…>` with the mock server + a throwaway swap target.
fn run_update(
    server: &MockServer,
    home: &Path,
    target: &Path,
    extra: &[&str],
) -> assert_cmd::assert::Assert {
    let mut cmd = cuvm();
    cmd.env("CUVM_HOME", home)
        .env("CUVM_SELF_UPDATE_API", server.base_url())
        .env("CUVM_DOWNLOAD_BASE", server.base_url())
        .env("CUVM_SELF_UPDATE_TARGET", target)
        .args(["self", "update"])
        .args(extra.iter().copied());
    cmd.assert()
}

#[test]
fn update_downloads_verifies_and_swaps_the_binary() {
    let home = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let script = "#!/bin/sh\necho \"cuvm 9.9.9\"\n";
    let (bytes, sha) = make_release(work.path(), "ok", "9.9.9", script);
    let server = mock_release("9.9.9", bytes, &sha);

    // Throwaway stand-in for the running binary — never touches the test binary.
    let target = work.child("bin/cuvm");
    target.write_str("OLD-BINARY").unwrap();

    run_update(&server, home.path(), target.path(), &[])
        .success()
        .stdout(contains("updated cuvm").and(contains("9.9.9")));

    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        script.as_bytes(),
        "the binary must be swapped for the release payload"
    );
    // Shims were refreshed into $CUVM_HOME/shims from the archive.
    assert!(home.child("shims/cuvm.sh").path().is_file());
}

#[test]
fn update_aborts_and_preserves_the_binary_on_checksum_mismatch() {
    let home = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let (bytes, _real_sha) = make_release(work.path(), "bad", "9.9.9", "#!/bin/sh\ntrue\n");
    // SHA256SUMS advertises the WRONG digest, so the verified download must fail.
    let server = mock_release("9.9.9", bytes, &"0".repeat(64));

    let target = work.child("bin/cuvm");
    target.write_str("OLD-BINARY").unwrap();

    run_update(&server, home.path(), target.path(), &[]).failure();

    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        b"OLD-BINARY",
        "a checksum mismatch must leave the installed binary untouched"
    );
}

#[test]
fn update_aborts_and_preserves_the_binary_when_the_new_binary_fails_its_smoke_test() {
    // Valid, correctly-hashed archive, but the binary exits non-zero on --version
    // (stands in for a corrupt or wrong-arch build). The smoke test must abort
    // BEFORE the swap — this is the headline "a bad build can never brick you".
    let home = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let (bytes, sha) = make_release(work.path(), "smoke", "9.9.9", "#!/bin/sh\nexit 1\n");
    let server = mock_release("9.9.9", bytes, &sha);

    let target = work.child("bin/cuvm");
    target.write_str("OLD-BINARY").unwrap();

    run_update(&server, home.path(), target.path(), &[]).failure();

    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        b"OLD-BINARY",
        "a binary that fails its smoke test must not be swapped in"
    );
}

#[test]
fn update_aborts_and_preserves_the_binary_when_the_archive_lacks_the_inner_binary() {
    // The archive verifies but contains no `<stage>/cuvm` (e.g. layout drift):
    // the missing-binary guard must abort before any swap.
    let home = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let (bytes, sha) = make_targz(
        work.path(),
        "nobin",
        "9.9.9",
        &[("shims/cuvm.sh", "# only\n")],
    );
    let server = mock_release("9.9.9", bytes, &sha);

    let target = work.child("bin/cuvm");
    target.write_str("OLD-BINARY").unwrap();

    run_update(&server, home.path(), target.path(), &[]).failure();

    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        b"OLD-BINARY",
        "an archive missing the binary must leave the install untouched"
    );
}

#[test]
fn update_to_an_explicit_older_version_with_force_swaps_the_binary() {
    // `--version <older> --force` is the documented rollback path: it must fetch
    // and swap in that specific version (running version is 0.2.0 > 0.1.0).
    let home = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let script = "#!/bin/sh\necho \"cuvm 0.1.0\"\n";
    let (bytes, sha) = make_release(work.path(), "rollback", "0.1.0", script);
    let server = mock_release("0.1.0", bytes, &sha);

    let target = work.child("bin/cuvm");
    target.write_str("CURRENT-BINARY").unwrap();

    run_update(
        &server,
        home.path(),
        target.path(),
        &["--version", "0.1.0", "--force"],
    )
    .success();

    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        script.as_bytes(),
        "--version --force must roll back to the requested release payload"
    );
}

#[test]
fn update_to_an_older_version_without_force_is_a_no_op() {
    // `--version <older>` without --force must not download or swap anything —
    // it resolves nothing over the network and leaves the binary intact.
    let home = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let target = work.child("bin/cuvm");
    target.write_str("CURRENT-BINARY").unwrap();

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_SELF_UPDATE_TARGET", target.path())
        .args(["self", "update", "--version", "0.1.0"])
        .assert()
        .success()
        .stdout(contains("at or ahead"));

    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        b"CURRENT-BINARY",
        "a no-op update must not touch the binary"
    );
}

#[test]
fn check_with_an_explicit_older_version_does_not_call_it_latest() {
    // `--check --version <older>` must report the requested version plainly, not
    // mislabel it as "latest", and must not download anything (no server needed).
    let home = TempDir::new().unwrap();
    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["self", "update", "--check", "--version", "0.1.0"])
        .assert()
        .success()
        .stdout(contains("at or ahead").and(contains("latest").not()));
}

#[test]
fn check_reports_a_newer_release_without_downloading() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    // Only the API is mocked; --check must not hit any download endpoint.
    server.mock(|when, then| {
        when.method(GET).path("/releases/latest");
        then.status(200).body("{\"tag_name\":\"v9.9.9\"}");
    });

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_SELF_UPDATE_API", server.base_url())
        .args(["self", "update", "--check"])
        .assert()
        .success()
        .stdout(contains("available"));
}
