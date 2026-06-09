//! Shared error type for the download + extract surface of `cuvm-download`.
//! Leaf crate => `thiserror` (no `anyhow` here; the app/cli edge maps these).

use std::path::PathBuf;

/// Failure modes for fetching, verifying, and (WU-12) extracting artifacts.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// A network/transport error from `ureq` (DNS, TLS, connection, timeout).
    #[error("transport error fetching {url}: {source}")]
    Transport {
        /// The URL whose transport failed.
        url: String,
        /// The underlying `ureq` transport error.
        #[source]
        source: Box<ureq::Error>,
    },

    /// The server answered, but with a non-success status.
    #[error("unexpected HTTP status {status} for {url}")]
    HttpStatus {
        /// The non-2xx status code returned.
        status: u16,
        /// The URL that produced the status.
        url: String,
    },

    /// The downloaded bytes did not match the manifest `sha256`. Nothing is kept.
    #[error("sha256 mismatch for {file_name}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// The cache file name whose digest failed to verify.
        file_name: String,
        /// The expected hex digest from the manifest.
        expected: String,
        /// The hex digest actually computed over the downloaded bytes.
        actual: String,
    },

    /// A filesystem error while reading, writing, or renaming a cache file.
    #[error("io error for {path}: {source}")]
    Io {
        /// The path whose I/O operation failed.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, DownloadError>;

#[cfg(test)]
mod tests {
    use super::DownloadError;

    #[test]
    fn checksum_mismatch_renders_both_hashes() {
        let e = DownloadError::ChecksumMismatch {
            file_name: "cuda_nvcc.tar.xz".into(),
            expected: "aaaa".into(),
            actual: "bbbb".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("cuda_nvcc.tar.xz"), "{msg}");
        assert!(msg.contains("aaaa") && msg.contains("bbbb"), "{msg}");
    }

    #[test]
    fn http_status_variant_carries_code_and_url() {
        let e = DownloadError::HttpStatus {
            status: 404,
            url: "http://x/y".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("404") && msg.contains("http://x/y"), "{msg}");
    }
}
