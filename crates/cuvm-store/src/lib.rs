//! cuvm-store — atomic manifest/.cuvm-meta I/O + content-addressed cudnn store.
//!
//! Real I/O lands in WU-3. WU-0 placeholder only.

#![forbid(unsafe_code)]

/// Scaffold marker. Replaced by atomic store I/O in WU-3.
#[must_use]
pub fn placeholder() -> &'static str {
    "cuvm-store"
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_names_the_crate() {
        assert_eq!(super::placeholder(), "cuvm-store");
    }
}
