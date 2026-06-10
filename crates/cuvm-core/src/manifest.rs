//! Serde state types: on-disk `manifest.json` and per-version `.cuvm-meta.json`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Source;

/// Bump when an incompatible on-disk change ships. Reader rejects anything higher.
pub const SCHEMA_VERSION: u32 = 1;

/// Root document at `$CUVM_HOME/manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    #[serde(default)]
    pub bundles: Vec<BundleRecord>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_driver: Option<DriverRecord>,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            schema_version: SCHEMA_VERSION,
            bundles: Vec::new(),
            aliases: BTreeMap::new(),
            pins: BTreeMap::new(),
            last_driver: None,
        }
    }
}

/// One installed/adopted bundle row in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleRecord {
    pub version: String,
    pub source: Source,
    /// Absolute external path for `Adopted`; `versions/<ver>` (relative to `CUVM_HOME`)
    /// for `Downloaded`/`Supplied`.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cudnn: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub installed_at: OffsetDateTime,
}

/// Sidecar at `$CUVM_HOME/versions/<ver>/.cuvm-meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionMeta {
    pub version: String,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cudnn: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub has_lib64: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub installed_at: OffsetDateTime,
}

/// Per-version cuDNN sidecar (`versions/<ver>/.cuvm-cudnn.json`) — the rich
/// record backing `Bundle.cudnn`. The manifest's `BundleRecord.cudnn` keeps
/// only the version string (D6: no manifest schema bump); the store path is
/// derived at hydration time as `<cudnn_dir>/<sha256>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CudnnRecord {
    pub version: String,
    pub cuda_major: u32,
    pub source: Source,
    pub sha256: String,
    #[serde(default)]
    pub libs: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub installed_at: OffsetDateTime,
}

/// Last driver probe cached in the manifest for offline `doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverRecord {
    pub version: String,
    pub cuda_ceiling: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use time::macros::datetime;

    fn sample() -> Manifest {
        let mut aliases = BTreeMap::new();
        aliases.insert("default".to_string(), "12.4.1".to_string());
        aliases.insert("lts".to_string(), "11.8.0".to_string());
        let mut pins = BTreeMap::new();
        pins.insert("/home/u/proj".to_string(), "12.4".to_string());
        Manifest {
            schema_version: 1,
            bundles: vec![BundleRecord {
                version: "12.4.1".to_string(),
                source: crate::Source::Downloaded,
                path: "/home/u/.cuvm/versions/12.4.1".to_string(),
                cudnn: Some("9.8.0".to_string()),
                components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
                sha256: Some("abc123".to_string()),
                installed_at: datetime!(2026-06-08 10:30:00 UTC),
            }],
            aliases,
            pins,
            last_driver: Some(DriverRecord {
                version: "550.54.14".to_string(),
                cuda_ceiling: "12.4".to_string(),
            }),
        }
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let m = sample();
        let json = serde_json::to_string_pretty(&m).unwrap();
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn manifest_json_uses_expected_field_names() {
        let m = sample();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"last_driver\""));
        assert!(json.contains("\"cuda_ceiling\":\"12.4\""));
        // Source serialized lowercase via domain::Source.
        assert!(json.contains("\"source\":\"downloaded\""));
    }

    #[test]
    fn aliases_serialize_in_btreemap_sorted_order() {
        let json = serde_json::to_string(&sample()).unwrap();
        // "default" sorts before "lts".
        let d = json.find("\"default\"").unwrap();
        let l = json.find("\"lts\"").unwrap();
        assert!(
            d < l,
            "BTreeMap must emit aliases sorted for golden stability"
        );
    }

    #[test]
    fn version_meta_round_trips() {
        let vm = VersionMeta {
            version: "13.3.0".to_string(),
            source: crate::Source::Downloaded,
            cudnn: None,
            components: vec!["cuda_nvcc".into(), "cuda_crt".into(), "cccl".into()],
            sha256: None,
            has_lib64: false,
            installed_at: datetime!(2026-06-08 11:00:00 UTC),
        };
        let json = serde_json::to_string(&vm).unwrap();
        let back: VersionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(vm, back);
        assert!(json.contains("\"has_lib64\":false"));
    }

    #[test]
    fn cudnn_record_round_trips() {
        let rec = CudnnRecord {
            version: "9.8.0".into(),
            cuda_major: 12,
            source: Source::Downloaded,
            sha256: "feed".into(),
            libs: vec!["libcudnn.so".into(), "libcudnn_ops.so".into()],
            installed_at: time::macros::datetime!(2026-06-10 10:30:00 UTC),
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back: CudnnRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rec);
    }
}
