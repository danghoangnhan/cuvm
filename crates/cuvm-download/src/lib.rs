//! cuvm-download — ureq+rustls fetch, sha256, tar.xz / zip extract (zip-slip guard).
//!
//! Module map (kept stable so WU-12's `extract` module slots in cleanly):
//! - [`error`]    — the shared [`DownloadError`].
//! - [`http`]     — bare `http_get` for small JSON/HTML.
//! - [`download`] — `sha256_file` + the resumable, verifying [`Downloader`].
//! - [`extract`]  — `tar.xz` / `zip` extraction with a shared zip-slip guard.

#![forbid(unsafe_code)]

pub mod download;
pub mod error;
pub mod extract;
pub mod http;
pub mod progress;

pub use download::{sha256_file, Downloader};
pub use error::{DownloadError, Result};
pub use extract::{extract_tar_gz, extract_tar_xz, extract_zip, strip_wrapper_dir, ExtractError};
pub use http::http_get;
pub use progress::{silent, ProgressReporter, Reporter, SilentReporter};
