//! cuvm-download — ureq+rustls fetch, sha256, tar.xz / zip extract (zip-slip guard).
//!
//! Real downloader/extractor lands in WU-11/WU-12. WU-0 placeholder only.

#![forbid(unsafe_code)]

/// Scaffold marker. Replaced by the downloader in WU-11.
#[must_use]
pub fn placeholder() -> &'static str {
    "cuvm-download"
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_names_the_crate() {
        assert_eq!(super::placeholder(), "cuvm-download");
    }
}
