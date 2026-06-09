//! Bare HTTP GET for small JSON/HTML bodies (registry index + redistrib manifests).
//! Blocking `ureq`+`rustls`, no async runtime. Artifacts use [`crate::Downloader`].

use std::io::Read;

use crate::error::{DownloadError, Result};

/// Fetch a small body in full and return it as bytes.
///
/// Intended for JSON/HTML metadata, not multi-megabyte artifacts; the body is
/// read entirely into memory (capped at 64 `MiB` to bound a hostile response).
///
/// # Errors
/// - [`DownloadError::HttpStatus`] if the server answers with a non-2xx status.
/// - [`DownloadError::Transport`] for DNS/TLS/connection/timeout failures.
/// - [`DownloadError::Io`] if reading the response body fails.
pub fn http_get(url: &str) -> Result<Vec<u8>> {
    const MAX_BODY: u64 = 64 * 1024 * 1024;

    match ureq::get(url).call() {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .take(MAX_BODY)
                .read_to_end(&mut buf)
                .map_err(|source| DownloadError::Io {
                    path: std::path::PathBuf::from(url),
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
