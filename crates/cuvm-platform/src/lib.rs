//! cuvm-platform — per-OS Activator + Installer backends.
//!
//! `#[cfg(unix)]` / `#[cfg(windows)]` syscall floors and the
//! `new_activator` / `new_installer` runtime factories land in WU-1+.

/// Scaffold marker. Replaced by per-OS backends in WU-1+.
pub fn placeholder() -> &'static str {
    "cuvm-platform"
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_names_the_crate() {
        assert_eq!(super::placeholder(), "cuvm-platform");
    }
}
