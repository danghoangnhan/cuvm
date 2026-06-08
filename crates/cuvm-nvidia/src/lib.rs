//! cuvm-nvidia — nvidia-smi driver probe (graceful-absent).
//!
//! Real `DriverProbe` impl lands in WU-7-adjacent work. WU-0 placeholder only.

#![forbid(unsafe_code)]

/// Scaffold marker. Replaced by the nvidia-smi probe later.
#[must_use]
pub fn placeholder() -> &'static str {
    "cuvm-nvidia"
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_names_the_crate() {
        assert_eq!(super::placeholder(), "cuvm-nvidia");
    }
}
