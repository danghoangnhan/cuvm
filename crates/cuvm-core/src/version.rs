use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::CoreError;

/// A dotted version compared **field-by-field numerically**, missing tail = 0.
/// `raw` preserves the original string for round-trip display.
#[derive(Debug, Clone)]
pub struct Version {
    pub fields: Vec<u32>,
    pub raw: String,
}

impl Version {
    /// Parse a dotted numeric version string (e.g. `"13.3.0"`).
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidVersion`] if the string is empty or any
    /// dot-separated field is not a non-negative integer.
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(CoreError::InvalidVersion { raw: s.to_string() });
        }
        let mut fields = Vec::new();
        for part in s.split('.') {
            let n: u32 = part
                .parse()
                .map_err(|_| CoreError::InvalidVersion { raw: s.to_string() })?;
            fields.push(n);
        }
        Ok(Version {
            fields,
            raw: s.to_string(),
        })
    }

    /// The first (major) field, or `0` if the version has no fields.
    #[must_use]
    pub fn major(&self) -> u32 {
        self.fields.first().copied().unwrap_or(0)
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let n = self.fields.len().max(other.fields.len());
        for i in 0..n {
            let a = self.fields.get(i).copied().unwrap_or(0);
            let b = other.fields.get(i).copied().unwrap_or(0);
            let ord = a.cmp(&b);
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.raw)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Version::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_numeric_fields_and_keeps_raw() {
        let v = Version::parse("13.3.0").unwrap();
        assert_eq!(v.fields, vec![13, 3, 0]);
        assert_eq!(v.raw, "13.3.0");
        assert_eq!(v.major(), 13);
    }

    #[test]
    fn parse_supports_four_part_cccl_version() {
        let v = Version::parse("13.3.3.3.1").unwrap();
        assert_eq!(v.fields, vec![13, 3, 3, 3, 1]);
        assert_eq!(v.major(), 13);
    }

    #[test]
    fn parse_rejects_empty_and_non_numeric() {
        assert!(Version::parse("").is_err());
        assert!(Version::parse("12.x").is_err());
        assert!(Version::parse("v12.4").is_err());
    }

    #[test]
    fn ord_is_numeric_not_lexical() {
        // 570.26 < 570.124.06 numerically; lexical compare would get this WRONG.
        let a = Version::parse("570.26").unwrap();
        let b = Version::parse("570.124.06").unwrap();
        assert!(a < b, "expected 570.26 < 570.124.06 numerically");
    }

    #[test]
    fn ord_treats_missing_tail_as_zero() {
        // 12.4 == 12.4.0 ; 12.4 < 12.4.1
        assert_eq!(
            Version::parse("12.4").unwrap(),
            Version::parse("12.4.0").unwrap()
        );
        assert!(Version::parse("12.4").unwrap() < Version::parse("12.4.1").unwrap());
    }

    #[test]
    fn eq_ignores_raw_string_differences() {
        // 12.04 and 12.4 compare equal (numeric); raw is preserved separately.
        let a = Version::parse("12.04").unwrap();
        let b = Version::parse("12.4").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.raw, "12.04");
    }

    #[test]
    fn display_renders_raw() {
        assert_eq!(Version::parse("12.4.1").unwrap().to_string(), "12.4.1");
    }
}
