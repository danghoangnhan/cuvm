//! TTL'd, platform-scoped cache of the remote redist toolkit index, so `cuvm ls`
//! can show `<download available>` rows without touching the network. The cache
//! is refreshed only by the network commands (install / ls --only-downloads /
//! ls-remote / ls --refresh).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use cuvm_core::{Platform, Version};

use crate::atomic::write_atomic;
use crate::error::Result;
use crate::layout::Layout;

/// Bump when the cache file layout changes incompatibly.
const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheDoc {
    schema_version: u32,
    os: String,
    arch: String,
    #[serde(with = "time::serde::rfc3339")]
    fetched_at: OffsetDateTime,
    versions: Vec<String>,
}

/// `<home>/cache/redist-index.json`.
#[must_use]
pub fn cache_path(layout: &Layout) -> PathBuf {
    layout.root().join("cache").join("redist-index.json")
}

fn platform_tag(p: Platform) -> (String, String) {
    (
        format!("{:?}", p.os).to_lowercase(),
        format!("{:?}", p.arch).to_lowercase(),
    )
}

/// Read cached available versions for `platform`. Returns `None` (never an error)
/// when the cache is missing, unreadable, schema-mismatched, for a different
/// platform, or older than `ttl_secs` relative to `now`.
#[must_use]
pub fn read(
    layout: &Layout,
    platform: &Platform,
    now: OffsetDateTime,
    ttl_secs: i64,
) -> Option<Vec<Version>> {
    let bytes = std::fs::read(cache_path(layout)).ok()?;
    let doc: CacheDoc = serde_json::from_slice(&bytes).ok()?;
    if doc.schema_version != SCHEMA {
        return None;
    }
    let (os, arch) = platform_tag(*platform);
    if doc.os != os || doc.arch != arch {
        return None;
    }
    if (now - doc.fetched_at).whole_seconds() > ttl_secs {
        return None;
    }
    Some(
        doc.versions
            .iter()
            .filter_map(|r| Version::parse(r).ok())
            .collect(),
    )
}

/// Write `versions` to the platform-scoped cache, stamped at `now`.
///
/// # Errors
/// Returns [`crate::error::StoreError::Io`] if the cache file cannot be written.
///
/// # Panics
/// Panics only if the (infallible) `CacheDoc` serialization fails, which cannot
/// happen for this all-owned, plain-data document.
pub fn write(
    layout: &Layout,
    platform: &Platform,
    versions: &[Version],
    now: OffsetDateTime,
) -> Result<()> {
    let (os, arch) = platform_tag(*platform);
    let doc = CacheDoc {
        schema_version: SCHEMA,
        os,
        arch,
        fetched_at: now,
        versions: versions.iter().map(|v| v.raw.clone()).collect(),
    };
    let json = serde_json::to_vec_pretty(&doc).expect("CacheDoc is always serializable");
    write_atomic(&cache_path(layout), &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::{Arch, Os, Platform};
    use time::Duration;

    fn plat() -> Platform {
        Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        }
    }

    #[test]
    fn round_trips_within_ttl() {
        let tmp = assert_fs::TempDir::new().unwrap();
        let layout = Layout::new(tmp.path());
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let vers = vec![
            Version::parse("12.6.0").unwrap(),
            Version::parse("12.4.1").unwrap(),
        ];
        write(&layout, &plat(), &vers, now).unwrap();
        let got = read(&layout, &plat(), now, 86_400).unwrap();
        assert_eq!(got, vers);
    }

    #[test]
    fn stale_cache_reads_as_none() {
        let tmp = assert_fs::TempDir::new().unwrap();
        let layout = Layout::new(tmp.path());
        let written = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        write(
            &layout,
            &plat(),
            &[Version::parse("12.4.1").unwrap()],
            written,
        )
        .unwrap();
        let later = written + Duration::seconds(86_401);
        assert!(read(&layout, &plat(), later, 86_400).is_none());
    }

    #[test]
    fn other_platform_reads_as_none() {
        let tmp = assert_fs::TempDir::new().unwrap();
        let layout = Layout::new(tmp.path());
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        write(&layout, &plat(), &[Version::parse("12.4.1").unwrap()], now).unwrap();
        let win = Platform {
            os: Os::Windows,
            arch: Arch::X86_64,
        };
        assert!(read(&layout, &win, now, 86_400).is_none());
    }

    #[test]
    fn missing_cache_reads_as_none() {
        let tmp = assert_fs::TempDir::new().unwrap();
        let layout = Layout::new(tmp.path());
        let now = OffsetDateTime::UNIX_EPOCH;
        assert!(read(&layout, &plat(), now, 86_400).is_none());
    }
}
