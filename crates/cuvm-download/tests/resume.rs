use cuvm_download::{sha256_file, Downloader};
use httpmock::prelude::*;
use std::path::Path;

const BODY: &[u8] = b"0123456789ABCDEFGHIJ"; // 20 bytes
const HEAD_LEN: usize = 8; // pre-seeded into the .part

fn body_sha(dir: &Path) -> String {
    let p = dir.join("ref.bin");
    std::fs::write(&p, BODY).unwrap();
    sha256_file(&p).unwrap()
}

#[test]
fn fetch_resumes_from_existing_part_via_range() {
    let cache = assert_fs::TempDir::new().unwrap();
    let sha = body_sha(cache.path());

    // Interrupted prior run: first HEAD_LEN bytes already on disk.
    std::fs::write(cache.path().join("r.tar.xz.part"), &BODY[..HEAD_LEN]).unwrap();

    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET)
            .path("/r.tar.xz")
            .header("range", format!("bytes={HEAD_LEN}-"));
        then.status(206)
            .header("content-range", format!("bytes {HEAD_LEN}-19/20"))
            .body(&BODY[HEAD_LEN..]);
    });

    let dl = Downloader::new(cache.path().to_path_buf());
    let out = dl
        .fetch(&server.url("/r.tar.xz"), &sha, "r.tar.xz")
        .unwrap();
    m.assert(); // the Range request was actually made

    assert_eq!(std::fs::read(&out).unwrap(), BODY);
    assert!(!cache.path().join("r.tar.xz.part").exists());
}

#[test]
fn fetch_resume_falls_back_to_full_body_on_200() {
    let cache = assert_fs::TempDir::new().unwrap();
    let sha = body_sha(cache.path());

    // A stale/garbage .part that a non-Range server would otherwise corrupt.
    std::fs::write(cache.path().join("s.tar.xz.part"), b"STALEDATA").unwrap();

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/s.tar.xz");
        then.status(200).body(BODY); // server ignored Range -> full body
    });

    let dl = Downloader::new(cache.path().to_path_buf());
    let out = dl
        .fetch(&server.url("/s.tar.xz"), &sha, "s.tar.xz")
        .unwrap();

    assert_eq!(std::fs::read(&out).unwrap(), BODY);
    assert!(!cache.path().join("s.tar.xz.part").exists());
}
