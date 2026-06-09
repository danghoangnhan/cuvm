//! sha256 verification + the resumable, verifying [`Downloader`].
//! Blocking `ureq`+`rustls`; resumable via HTTP `Range`; `sha2` for hashing.

use std::fs::File;
use std::io::Read;
use std::path::Path;

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
