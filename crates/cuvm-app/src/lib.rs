//! cuvm-app — use-cases and trait ports (Resolver, Activator, Installer, ...).
//!
//! Trait ports land in WU-1. This placeholder keeps the crate building and
//! asserts the core dependency edge is wired.

#![forbid(unsafe_code)]

/// Scaffold marker. Replaced by trait ports in WU-1.
/// Returns `String` (allocates) intentionally to exercise the `cuvm-core` dependency edge.
#[must_use]
pub fn placeholder() -> String {
    format!("cuvm-app over {}", cuvm_core::placeholder())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_wraps_core() {
        assert_eq!(placeholder(), "cuvm-app over cuvm-core");
    }
}
