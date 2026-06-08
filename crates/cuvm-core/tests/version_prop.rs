use cuvm_core::Version;
use proptest::prelude::*;

/// Generate a dotted numeric string of 1..=5 components, each 0..=9999.
fn version_string() -> impl Strategy<Value = String> {
    prop::collection::vec(0u32..10_000, 1..=5)
        .prop_map(|parts| parts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("."))
}

proptest! {
    #[test]
    fn parse_then_display_then_parse_is_identity(s in version_string()) {
        let a = Version::parse(&s).expect("generated string parses");
        // Display renders `raw`, which equals the source string.
        prop_assert_eq!(a.to_string(), s.clone());
        let b = Version::parse(&a.to_string()).expect("reparse");
        // Numeric equality (tail-zero tolerant) holds across the round trip.
        prop_assert_eq!(a, b);
    }

    #[test]
    fn ord_is_total_and_antisymmetric(s1 in version_string(), s2 in version_string()) {
        let a = Version::parse(&s1).unwrap();
        let b = Version::parse(&s2).unwrap();
        // Exactly one of <, ==, > holds, and it is symmetric under swap.
        let f = a.cmp(&b);
        let r = b.cmp(&a);
        prop_assert_eq!(f, r.reverse());
    }
}
