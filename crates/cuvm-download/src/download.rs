//! sha256 verification + the resumable, verifying [`Downloader`].
//! Blocking `ureq`+`rustls`; resumable via HTTP `Range`; `sha2` for hashing.

use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{DownloadError, Result};

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
#[derive(Debug, Clone)]
pub struct Downloader {
    cache_dir: PathBuf,
}

impl Downloader {
    /// Create a downloader writing into `cache_dir` (created on first `fetch`).
    #[must_use]
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
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

        // --- DOWNLOAD-INTO-PART SEAM (Task 11.5 inserts the Range-resume branch
        // --- here; for now we always (re)start at byte 0). ---
        let body = http_body(url)?;
        let mut part = fs::File::create(&part_path).map_err(|source| DownloadError::Io {
            path: part_path.clone(),
            source,
        })?;
        part.write_all(&body).map_err(|source| DownloadError::Io {
            path: part_path.clone(),
            source,
        })?;
        part.flush().map_err(|source| DownloadError::Io {
            path: part_path.clone(),
            source,
        })?;
        drop(part);
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
        Ok(final_path)
    }
}

/// Fetch the full body of an artifact URL. Split out so the resume path (Task
/// 11.5) can wrap it with a `Range` request without touching `fetch`'s control flow.
fn http_body(url: &str) -> Result<Vec<u8>> {
    match ureq::get(url).call() {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .read_to_end(&mut buf)
                .map_err(|source| DownloadError::Io {
                    path: PathBuf::from(url),
                    source,
                })?;
            Ok(buf)
        }
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
