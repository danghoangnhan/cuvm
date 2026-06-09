//! cuvm-download — ureq+rustls fetch, sha256, tar.xz / zip extract (zip-slip guard).
//!
//! Module map (kept stable so WU-12's `extract` module slots in cleanly):
//! - [`error`]    — the shared [`DownloadError`].
//! - [`http`]     — bare `http_get` for small JSON/HTML.
//! - [`download`] — `sha256_file` + the resumable, verifying [`Downloader`].

#![forbid(unsafe_code)]

pub mod download;
pub mod error;
pub mod http;

pub use error::{DownloadError, Result};
