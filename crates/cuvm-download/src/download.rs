//! sha256 verification + the resumable, verifying [`Downloader`].
//! Blocking `ureq`+`rustls`; resumable via HTTP `Range`; `sha2` for hashing.

use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{DownloadError, Result};
use crate::progress::{silent, Reporter};

/// Stream a file through SHA-256 and return its lowercase hex digest.
///
/// Reads in 64 `KiB` chunks so an artifact of any size hashes in constant memory.
///
/// # Errors
/// Returns [`DownloadError::Io`] if the file cannot be opened or read.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|source| DownloadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024].into_boxed_slice();
    loop {
        let n = file.read(&mut buf).map_err(|source| DownloadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Lowercase-hex-encode a byte slice without pulling in a `hex` dependency.
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// A content-addressed download cache. `fetch` is resumable and sha256-verified;
/// a re-fetch of an already-complete, already-correct file is a no-op.
#[derive(Clone)]
pub struct Downloader {
    cache_dir: PathBuf,
    reporter: Reporter,
}

impl Downloader {
    /// Create a downloader with the silent (no-op) reporter.
    ///
    /// Writes into `cache_dir` (created on first `fetch`).
    #[must_use]
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            reporter: silent(),
        }
    }

    /// Create a downloader that reports progress to `reporter`.
    #[must_use]
    pub fn with_reporter(cache_dir: PathBuf, reporter: Reporter) -> Self {
        Self {
            cache_dir,
            reporter,
        }
    }

    /// Download `url` into `cache_dir/<file_name>`, verifying it matches
    /// `expected_sha256` before it is exposed under its final name.
    ///
    /// Bytes land in a `<file_name>.part` sidecar first; only a verified `.part`
    /// is atomically renamed to the final path. A digest mismatch deletes the
    /// `.part` and errors, keeping nothing. If the final file already exists and
    /// already matches `expected_sha256`, this returns immediately without any
    /// network access.
    ///
    /// # Errors
    /// - [`DownloadError::ChecksumMismatch`] if the downloaded bytes do not match.
    /// - [`DownloadError::HttpStatus`] / [`DownloadError::Transport`] on a bad
    ///   response or transport failure.
    /// - [`DownloadError::Io`] if a cache file cannot be created, written, or renamed.
    pub fn fetch(&self, url: &str, expected_sha256: &str, file_name: &str) -> Result<PathBuf> {
        self.fetch_labeled(url, expected_sha256, file_name, file_name)
    }

    /// Like [`Downloader::fetch`], but progress is reported under `label`
    /// (spec §5.4: `<component> <version>`) instead of the cache file name.
    ///
    /// # Errors
    /// Same as [`Downloader::fetch`].
    pub fn fetch_labeled(
        &self,
        url: &str,
        expected_sha256: &str,
        file_name: &str,
        label: &str,
    ) -> Result<PathBuf> {
        let final_path = self.cache_dir.join(file_name);
        let part_path = self.cache_dir.join(format!("{file_name}.part"));

        // No-op fast path: a complete, already-correct cached file.
        if final_path.is_file() && sha256_file(&final_path)? == expected_sha256 {
            return Ok(final_path);
        }

        fs::create_dir_all(&self.cache_dir).map_err(|source| DownloadError::Io {
            path: self.cache_dir.clone(),
            source,
        })?;

        // Resume if a .part survives a prior run. Failures up to (and including)
        // the request itself precede on_download_start, so they return plainly
        // without any terminal progress event.
        let resume_from = fs::metadata(&part_path).map_or(0, |m| m.len());
        let resp = open_response(url, resume_from)?;

        // 206 => append the tail to the existing .part, so the full size is the
        // tail's Content-Length plus the resumed prefix; anything else (200,
        // server ignored Range) => rewrite, so Content-Length is already full.
        let append = resp.status() == 206 && resume_from > 0;
        let total = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok())
            .map(|len| if append { len + resume_from } else { len });
        self.reporter.on_download_start(label, total);
        if append {
            // The resumed prefix is already on disk; account for it up front.
            self.reporter.on_download_advance(label, resume_from);
        }

        // The reporter saw a start: terminate with exactly one of finish
        // (success) or abort (any failure), so a bar never dangles (spec §6.4).
        match self.stream_verify_publish(resp, append, expected_sha256, file_name, label) {
            Ok(path) => {
                self.reporter.on_download_finish(label);
                Ok(path)
            }
            Err(err) => {
                self.reporter.on_download_abort(label);
                Err(err)
            }
        }
    }

    /// Download `url` into `cache_dir/<file_name>` WITHOUT a checksum (the
    /// caller self-records the sha256 — the NCCL redist publishes none, spec
    /// §2.3). Length is the ONLY integrity signal, so a response without a
    /// usable `Content-Length` is refused outright, and a body whose length
    /// disagrees with it is rejected. Always downloads fresh (no resume):
    /// without a checksum a resumed prefix cannot be trusted.
    ///
    /// # Errors
    /// - [`DownloadError::MissingContentLength`] if the response advertises no
    ///   usable `Content-Length` (cuvm will not self-record over unverifiable bytes).
    /// - [`DownloadError::SizeMismatch`] if the body length disagrees with it.
    /// - [`DownloadError::HttpStatus`] / [`DownloadError::Transport`] on a bad
    ///   response or transport failure.
    /// - [`DownloadError::Io`] if a cache file cannot be created, written, or renamed.
    pub fn fetch_unverified(&self, url: &str, file_name: &str, label: &str) -> Result<PathBuf> {
        fs::create_dir_all(&self.cache_dir).map_err(|source| DownloadError::Io {
            path: self.cache_dir.clone(),
            source,
        })?;
        // Always fresh (no resume): with no checksum a resumed prefix can't be
        // trusted. Refuse before announcing a start (so no bar dangles) when the
        // response carries no usable length — the sole integrity signal.
        let resp = open_response(url, 0)?;
        let Some(total) = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok())
        else {
            return Err(DownloadError::MissingContentLength {
                url: url.to_string(),
            });
        };
        self.reporter.on_download_start(label, Some(total));

        match self.stream_size_check_publish(resp, total, file_name, label) {
            Ok(path) => {
                self.reporter.on_download_finish(label);
                Ok(path)
            }
            Err(err) => {
                self.reporter.on_download_abort(label);
                Err(err)
            }
        }
    }

    /// Stream `resp` into the `.part`, check the byte count against `total` (the
    /// advertised `Content-Length`), and atomically expose the file — or keep
    /// nothing. The no-checksum sibling of [`Downloader::stream_verify_publish`].
    fn stream_size_check_publish(
        &self,
        resp: ureq::Response,
        total: u64,
        file_name: &str,
        label: &str,
    ) -> Result<PathBuf> {
        let final_path = self.cache_dir.join(file_name);
        let part_path = self.cache_dir.join(format!("{file_name}.part"));

        stream_into_part(resp, &part_path, false, self.reporter.as_ref(), label)?;

        let got = fs::metadata(&part_path)
            .map_err(|source| DownloadError::Io {
                path: part_path.clone(),
                source,
            })?
            .len();
        if got != total {
            let _ = fs::remove_file(&part_path);
            return Err(DownloadError::SizeMismatch {
                file_name: file_name.to_string(),
                expected: total,
                actual: got,
            });
        }

        fs::rename(&part_path, &final_path).map_err(|source| DownloadError::Io {
            path: final_path.clone(),
            source,
        })?;
        Ok(final_path)
    }

    /// Stream `resp` into the `.part`, verify the digest, and atomically expose
    /// the file under its final name — or keep nothing. Split out of
    /// [`Downloader::fetch_labeled`] so every `?` here funnels through its
    /// single finish/abort bracket.
    fn stream_verify_publish(
        &self,
        resp: ureq::Response,
        append: bool,
        expected_sha256: &str,
        file_name: &str,
        label: &str,
    ) -> Result<PathBuf> {
        let final_path = self.cache_dir.join(file_name);
        let part_path = self.cache_dir.join(format!("{file_name}.part"));

        stream_into_part(resp, &part_path, append, self.reporter.as_ref(), label)?;

        let actual = sha256_file(&part_path)?;
        if actual != expected_sha256 {
            let _ = fs::remove_file(&part_path);
            return Err(DownloadError::ChecksumMismatch {
                file_name: file_name.to_string(),
                expected: expected_sha256.to_string(),
                actual,
            });
        }

        fs::rename(&part_path, &final_path).map_err(|source| DownloadError::Io {
            path: final_path.clone(),
            source,
        })?;
        Ok(final_path)
    }
}

/// `GET url`, asking for `Range: bytes=<resume_from>-` when there is a `.part`
/// to resume. Returns the raw response; the caller decides append-vs-rewrite
/// from its status.
fn open_response(url: &str, resume_from: u64) -> Result<ureq::Response> {
    let req = ureq::get(url);
    let req = if resume_from > 0 {
        req.set("Range", &format!("bytes={resume_from}-"))
    } else {
        req
    };

    match req.call() {
        Ok(resp) => Ok(resp),
        Err(ureq::Error::Status(status, _resp)) => Err(DownloadError::HttpStatus {
            status,
            url: url.to_string(),
        }),
        Err(transport) => Err(DownloadError::Transport {
            url: url.to_string(),
            source: Box::new(transport),
        }),
    }
}

/// Stream the response body into `part_path`: append a `206` tail to the
/// existing `.part`, or (200, server ignored `Range`) truncate and write the
/// whole body so a stale `.part` can never corrupt the result.
fn stream_into_part(
    resp: ureq::Response,
    part_path: &Path,
    append: bool,
    reporter: &dyn crate::progress::ProgressReporter,
    label: &str,
) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(part_path)
        .map_err(|source| DownloadError::Io {
            path: part_path.to_path_buf(),
            source,
        })?;

    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 64 * 1024].into_boxed_slice();
    loop {
        let n = reader.read(&mut buf).map_err(|source| DownloadError::Io {
            path: part_path.to_path_buf(),
            source,
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|source| DownloadError::Io {
                path: part_path.to_path_buf(),
                source,
            })?;
        reporter.on_download_advance(label, n as u64);
    }
    file.flush().map_err(|source| DownloadError::Io {
        path: part_path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod sha_tests {
    use super::sha256_file;
    use std::io::Write;

    #[test]
    fn sha256_of_abc_matches_known_vector() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"abc").unwrap();
        f.flush().unwrap();
        let got = sha256_file(f.path()).unwrap();
        assert_eq!(
            got,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_of_empty_file_matches_known_vector() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let got = sha256_file(f.path()).unwrap();
        assert_eq!(
            got,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}

#[cfg(test)]
mod progress_tests {
    use super::Downloader;
    use crate::progress::recording::Recorder;
    use std::path::Path;
    use std::sync::Arc;

    use httpmock::prelude::*;
    use sha2::{Digest, Sha256};

    const BODY: &[u8] = b"0123456789ABCDEFGHIJ"; // 20 bytes
    const HEAD_LEN: usize = 8; // pre-seeded into the .part

    fn sha_of(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn recording_downloader(cache: &Path) -> (Downloader, Arc<Recorder>) {
        let rec = Arc::new(Recorder::default());
        let dl = Downloader::with_reporter(cache.to_path_buf(), rec.clone());
        (dl, rec)
    }

    /// Sum of all `advance:<label>:<n>` deltas the recorder saw.
    fn advance_sum(events: &[String], label: &str) -> u64 {
        let prefix = format!("advance:{label}:");
        events
            .iter()
            .filter_map(|e| e.strip_prefix(&prefix))
            .map(|n| n.parse::<u64>().unwrap())
            .sum()
    }

    #[test]
    fn fetch_reports_start_with_total_then_advance_then_finish() {
        let body = vec![7u8; 4096];
        let sha = sha_of(&body);
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/blob.bin");
            then.status(200).body(body.clone());
        });

        let cache = tempfile::tempdir().unwrap();
        let (dl, rec) = recording_downloader(cache.path());
        dl.fetch(&server.url("/blob.bin"), &sha, "blob.bin")
            .unwrap();

        let events = rec.events.lock().unwrap();
        // The start event must carry the exact Content-Length total.
        assert_eq!(
            events.first().map(String::as_str),
            Some("start:blob.bin:Some(4096)"),
            "{events:?}"
        );
        assert_eq!(advance_sum(&events, "blob.bin"), 4096, "{events:?}");
        assert_eq!(
            events.last().map(String::as_str),
            Some("finish:blob.bin"),
            "{events:?}"
        );
    }

    #[test]
    fn fetch_labeled_reports_progress_under_the_given_label() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/x.tar.xz");
            then.status(200).body(BODY);
        });

        let cache = tempfile::tempdir().unwrap();
        let (dl, rec) = recording_downloader(cache.path());
        let file_name = "cuda_cudart-linux-x86_64-12.4.131-archive.tar.xz";
        let out = dl
            .fetch_labeled(
                &server.url("/x.tar.xz"),
                &sha_of(BODY),
                file_name,
                "cuda_cudart 12.4.131",
            )
            .unwrap();

        // The cache file keeps the archive name; progress uses the §5.4 label.
        assert!(out.ends_with(file_name), "{}", out.display());
        let events = rec.events.lock().unwrap();
        assert!(
            !events.is_empty() && events.iter().all(|e| e.contains("cuda_cudart 12.4.131")),
            "{events:?}"
        );
        assert_eq!(
            events.last().map(String::as_str),
            Some("finish:cuda_cudart 12.4.131"),
            "{events:?}"
        );
    }

    #[test]
    fn resume_206_reports_full_total_and_advances_the_on_disk_prefix() {
        let cache = tempfile::tempdir().unwrap();
        // Interrupted prior run: first HEAD_LEN bytes already on disk.
        std::fs::write(cache.path().join("r.bin.part"), &BODY[..HEAD_LEN]).unwrap();

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/r.bin")
                .header("range", format!("bytes={HEAD_LEN}-"));
            then.status(206)
                .header("content-range", format!("bytes {HEAD_LEN}-19/20"))
                .body(&BODY[HEAD_LEN..]);
        });

        let (dl, rec) = recording_downloader(cache.path());
        dl.fetch(&server.url("/r.bin"), &sha_of(BODY), "r.bin")
            .unwrap();

        let events = rec.events.lock().unwrap();
        // Total is the FULL file size: the 206 tail's Content-Length + prefix…
        assert_eq!(
            events.first().map(String::as_str),
            Some(format!("start:r.bin:Some({})", BODY.len()).as_str()),
            "{events:?}"
        );
        // …and the resumed prefix is accounted for right after the start, so an
        // interactive bar reaches 100% instead of stalling at the tail size.
        assert_eq!(
            events.get(1).map(String::as_str),
            Some(format!("advance:r.bin:{HEAD_LEN}").as_str()),
            "{events:?}"
        );
        assert_eq!(
            advance_sum(&events, "r.bin"),
            BODY.len() as u64,
            "{events:?}"
        );
        assert_eq!(
            events.last().map(String::as_str),
            Some("finish:r.bin"),
            "{events:?}"
        );
    }

    #[test]
    fn resume_200_fallback_reports_full_length_total_without_double_count() {
        let cache = tempfile::tempdir().unwrap();
        // A stale .part the server ignores (200 => the body is the whole file).
        std::fs::write(cache.path().join("s.bin.part"), b"STALEDATA").unwrap();

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/s.bin");
            then.status(200).body(BODY);
        });

        let (dl, rec) = recording_downloader(cache.path());
        dl.fetch(&server.url("/s.bin"), &sha_of(BODY), "s.bin")
            .unwrap();

        let events = rec.events.lock().unwrap();
        // Content-Length is already the full size: no resume_from inflation and
        // no initial advance for the discarded stale prefix.
        assert_eq!(
            events.first().map(String::as_str),
            Some(format!("start:s.bin:Some({})", BODY.len()).as_str()),
            "{events:?}"
        );
        assert_eq!(
            advance_sum(&events, "s.bin"),
            BODY.len() as u64,
            "{events:?}"
        );
        assert_eq!(
            events.last().map(String::as_str),
            Some("finish:s.bin"),
            "{events:?}"
        );
    }

    #[test]
    fn checksum_mismatch_terminates_progress_with_abort_not_finish() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/bad.bin");
            then.status(200).body(BODY);
        });

        let cache = tempfile::tempdir().unwrap();
        let (dl, rec) = recording_downloader(cache.path());
        let err = dl
            .fetch(&server.url("/bad.bin"), &"00".repeat(32), "bad.bin")
            .unwrap_err();
        assert!(
            matches!(err, crate::DownloadError::ChecksumMismatch { .. }),
            "{err:?}"
        );

        let events = rec.events.lock().unwrap();
        assert_eq!(
            events.first().map(String::as_str),
            Some(format!("start:bad.bin:Some({})", BODY.len()).as_str()),
            "{events:?}"
        );
        assert_eq!(
            events.last().map(String::as_str),
            Some("abort:bad.bin"),
            "{events:?}"
        );
        assert!(
            !events.iter().any(|e| e.starts_with("finish:")),
            "{events:?}"
        );
    }

    #[test]
    fn mid_download_stream_failure_terminates_progress_with_abort() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/dir.bin");
            then.status(200).body(BODY);
        });

        let cache = tempfile::tempdir().unwrap();
        // A directory squatting on the .part path: the response opens fine, so
        // on_download_start fires, but streaming into the sidecar then fails.
        std::fs::create_dir(cache.path().join("dir.bin.part")).unwrap();

        let (dl, rec) = recording_downloader(cache.path());
        dl.fetch(&server.url("/dir.bin"), &sha_of(BODY), "dir.bin")
            .unwrap_err();

        let events = rec.events.lock().unwrap();
        assert!(
            events
                .first()
                .is_some_and(|e| e.starts_with("start:dir.bin:")),
            "{events:?}"
        );
        assert_eq!(
            events.last().map(String::as_str),
            Some("abort:dir.bin"),
            "{events:?}"
        );
        assert!(
            !events.iter().any(|e| e.starts_with("finish:")),
            "{events:?}"
        );
    }

    #[test]
    fn fetch_unverified_downloads_a_self_recordable_file() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/nccl.txz");
            then.status(200).body(BODY);
        });
        let cache = tempfile::tempdir().unwrap();
        let (dl, rec) = recording_downloader(cache.path());
        let path = dl
            .fetch_unverified(&server.url("/nccl.txz"), "nccl.txz", "nccl 2.21.5")
            .unwrap();
        // The bytes landed and hash to the expected digest (caller self-records).
        assert_eq!(std::fs::read(&path).unwrap(), BODY);
        assert_eq!(super::sha256_file(&path).unwrap(), sha_of(BODY));
        let events = rec.events.lock().unwrap();
        assert_eq!(
            events.last().map(String::as_str),
            Some("finish:nccl 2.21.5")
        );
    }

    // NOTE: the `SizeMismatch` (truncated body) branch of `fetch_unverified`
    // cannot be exercised through httpmock — its hyper backend panics if asked
    // to serve a `Content-Length` that disagrees with the body. The guard
    // itself is a trivial `if got != total`; the variant's message is locked by
    // a unit test in `error.rs`.

    #[test]
    fn http_status_failure_before_start_emits_no_progress_events() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/missing.bin");
            then.status(404);
        });

        let cache = tempfile::tempdir().unwrap();
        let (dl, rec) = recording_downloader(cache.path());
        dl.fetch(&server.url("/missing.bin"), &"00".repeat(32), "missing.bin")
            .unwrap_err();

        // No start was emitted, so no terminal abort/finish may follow either.
        assert!(rec.events.lock().unwrap().is_empty());
    }
}
