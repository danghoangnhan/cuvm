//! cuvm-registry — parse redistrib_<ver>.json (serde flatten, dynamic keys).
//!
//! Real parser lands in WU-10. WU-0 placeholder only.

/// Scaffold marker. Replaced by the redist parser in WU-10.
pub fn placeholder() -> &'static str {
    "cuvm-registry"
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_names_the_crate() {
        assert_eq!(super::placeholder(), "cuvm-registry");
    }
}
