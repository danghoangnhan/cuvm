//! cuvm-core — pure domain types and logic. Zero I/O dependencies.
//!
//! Real types (`Version`, `Bundle`, `EnvPlan`, compat tables, ...) land in
//! later work units. This placeholder keeps the crate building under WU-0.

/// Scaffold marker. Replaced by real domain types in WU-2+.
pub fn placeholder() -> &'static str {
    "cuvm-core"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_names_the_crate() {
        assert_eq!(placeholder(), "cuvm-core");
    }
}
