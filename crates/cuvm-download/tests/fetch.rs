use cuvm_download::{sha256_file, DownloadError, Downloader};
use httpmock::prelude::*;
use std::path::Path;

const BODY: &[u8] = b"cuvm-redist-artifact-bytes\n";
// sha256 of BODY (verified once and pinned; `sha256_file` recomputes it in-test).
fn body_sha(dir: &Path) -> String {
    let p = dir.join("ref.bin");
    std::fs::write(&p, BODY).unwrap();
    sha256_file(&p).unwrap()
}

#[test]
fn fetch_downloads_verifies_and_renames_to_final() {
    let cache = assert_fs::TempDir::new().unwrap();
    let sha = body_sha(cache.path());

    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET).path("/a.tar.xz");
        then.status(200).body(BODY);
    });

    let dl = Downloader::new(cache.path().to_path_buf());
    let out = dl
        .fetch(&server.url("/a.tar.xz"), &sha, "a.tar.xz")
        .unwrap();
    m.assert();

    assert_eq!(out, cache.path().join("a.tar.xz"));
    assert_eq!(std::fs::read(&out).unwrap(), BODY);
    assert!(
        !cache.path().join("a.tar.xz.part").exists(),
        ".part must be gone"
    );
}

#[test]
fn fetch_with_wrong_sha_deletes_part_and_errors() {
    let cache = assert_fs::TempDir::new().unwrap();

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/b.tar.xz");
        then.status(200).body(BODY);
    });

    let dl = Downloader::new(cache.path().to_path_buf());
    let err = dl
        .fetch(&server.url("/b.tar.xz"), "deadbeef", "b.tar.xz")
        .expect_err("wrong sha must error");

    match err {
        DownloadError::ChecksumMismatch {
            file_name,
            expected,
            ..
        } => {
            assert_eq!(file_name, "b.tar.xz");
            assert_eq!(expected, "deadbeef");
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
    // keep nothing: neither the final nor the .part survives.
    assert!(!cache.path().join("b.tar.xz").exists());
    assert!(!cache.path().join("b.tar.xz.part").exists());
}

#[test]
fn fetch_of_already_complete_file_is_a_noop() {
    let cache = assert_fs::TempDir::new().unwrap();
    let sha = body_sha(cache.path());

    // Pre-seed the final, already-correct file.
    std::fs::write(cache.path().join("c.tar.xz"), BODY).unwrap();

    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET).path("/c.tar.xz");
        then.status(200).body(BODY);
    });

    let dl = Downloader::new(cache.path().to_path_buf());
    let out = dl
        .fetch(&server.url("/c.tar.xz"), &sha, "c.tar.xz")
        .unwrap();

    assert_eq!(out, cache.path().join("c.tar.xz"));
    // No network hit: the mock was never called.
    m.assert_calls(0);
}
