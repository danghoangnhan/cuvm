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

        // --- DOWNLOAD-INTO-PART SEAM: resume if a .part survives a prior run. ---
        let resume_from = fs::metadata(&part_path).map_or(0, |m| m.len());
        http_into_part(
            url,
            &part_path,
            resume_from,
            self.reporter.as_ref(),
            file_name,
        )?;
        // --- END SEAM ---

        // Verify, then atomically expose under the final name — or keep nothing.
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
        self.reporter.on_download_finish(file_name);
        Ok(final_path)
    }
}

/// Stream `url` into `part_path`. If `resume_from > 0`, request `Range:
/// bytes=<resume_from>-` and append a `206` tail to the existing `.part`; on a
/// `200` (server ignored `Range`) truncate and write the whole body so a stale
/// `.part` can never corrupt the result.
fn http_into_part(
    url: &str,
    part_path: &Path,
    resume_from: u64,
    reporter: &dyn crate::progress::ProgressReporter,
    label: &str,
) -> Result<()> {
    let req = ureq::get(url);
    let req = if resume_from > 0 {
        req.set("Range", &format!("bytes={resume_from}-"))
    } else {
        req
    };

    let resp = match req.call() {
        Ok(resp) => resp,
        Err(ureq::Error::Status(status, _resp)) => {
            return Err(DownloadError::HttpStatus {
                status,
                url: url.to_string(),
            })
        }
        Err(transport) => {
            return Err(DownloadError::Transport {
                url: url.to_string(),
                source: Box::new(transport),
            })
        }
    };

    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .map(|len| len + resume_from);
    reporter.on_download_start(label, total);

    // 206 => append to the existing .part; anything else (200) => rewrite it.
    let append = resp.status() == 206 && resume_from > 0;
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
    use std::sync::Arc;

    #[test]
    fn fetch_reports_start_advance_finish() {
        use httpmock::prelude::*;
        use sha2::{Digest, Sha256};

        let body = vec![7u8; 4096];
        let sha = format!("{:x}", Sha256::digest(&body));
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/blob.bin");
            then.status(200).body(body.clone());
        });

        let cache = tempfile::tempdir().unwrap();
        let rec = Arc::new(Recorder::default());
        let dl = Downloader::with_reporter(cache.path().to_path_buf(), rec.clone());
        dl.fetch(&server.url("/blob.bin"), &sha, "blob.bin")
            .unwrap();

        let events = rec.events.lock().unwrap();
        assert!(
            events.iter().any(|e| e.starts_with("start:blob.bin")),
            "{events:?}"
        );
        assert!(
            events.iter().any(|e| e.starts_with("advance:blob.bin")),
            "{events:?}"
        );
        assert!(events.iter().any(|e| e == "finish:blob.bin"), "{events:?}");
    }
}
