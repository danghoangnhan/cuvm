//! Read/write `manifest.json` with schema guard and atomic save.

use std::fs;
use std::path::Path;

use cuvm_core::{Manifest, SCHEMA_VERSION};

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};

/// Read the manifest. Missing file => fresh `Manifest::default()`.
///
/// # Errors
/// Returns [`StoreError::Corrupt`] if the file contains invalid JSON,
/// [`StoreError::SchemaTooNew`] if `schema_version > SCHEMA_VERSION`, or
/// [`StoreError::Io`] for other I/O failures.
pub fn read_manifest(path: &Path) -> Result<Manifest> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Manifest::default());
        }
        Err(source) => {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    // Peek schema_version before full deserialize so a newer doc fails loudly,
    // not as a confusing field error.
    let probe: SchemaProbe =
        serde_json::from_slice(&bytes).map_err(|source| StoreError::Corrupt {
            path: path.to_path_buf(),
            source,
        })?;
    if probe.schema_version > SCHEMA_VERSION {
        return Err(StoreError::SchemaTooNew {
            path: path.to_path_buf(),
            found: probe.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    serde_json::from_slice(&bytes).map_err(|source| StoreError::Corrupt {
        path: path.to_path_buf(),
        source,
    })
}

/// Serialize pretty + atomically replace the manifest file.
///
/// # Errors
/// Returns [`StoreError::Io`] if the atomic write fails.
///
/// # Panics
/// Never panics — `Manifest` is always serializable.
pub fn write_manifest(path: &Path, m: &Manifest) -> Result<()> {
    let json = serde_json::to_vec_pretty(m).expect("Manifest is always serializable");
    write_atomic(path, &json)
}

#[derive(serde::Deserialize)]
struct SchemaProbe {
    #[serde(default)]
    schema_version: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::{BundleRecord, Source};
    use std::collections::BTreeMap;
    use time::macros::datetime;

    fn sample() -> Manifest {
        let mut aliases = BTreeMap::new();
        aliases.insert("default".to_string(), "12.4.1".to_string());
        Manifest {
            schema_version: SCHEMA_VERSION,
            bundles: vec![BundleRecord {
                version: "12.4.1".to_string(),
                source: Source::Downloaded,
                path: "versions/12.4.1".to_string(),
                cudnn: None,
                components: vec!["cuda_nvcc".to_string(), "cuda_cudart".to_string()],
                sha256: Some("abc".to_string()),
                installed_at: datetime!(2026-06-08 10:30:00 UTC),
            }],
            aliases,
            pins: BTreeMap::new(),
            last_driver: None,
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        write_manifest(&path, &sample()).unwrap();
        let back = read_manifest(&path).unwrap();
        assert_eq!(sample(), back);
    }

    #[test]
    fn absent_file_yields_default_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let m = read_manifest(&path).unwrap();
        assert_eq!(m, Manifest::default());
    }

    #[test]
    fn newer_schema_is_rejected_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        fs::write(&path, br#"{"schema_version":999,"bundles":[]}"#).unwrap();
        let err = read_manifest(&path).unwrap_err();
        assert!(matches!(
            err,
            StoreError::SchemaTooNew { found: 999, supported: 1, .. }
        ));
    }

    #[test]
    fn corrupt_json_is_typed_error_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        fs::write(&path, b"{ this is not json ]").unwrap();
        let err = read_manifest(&path).unwrap_err();
        assert!(matches!(err, StoreError::Corrupt { .. }));
    }

    #[test]
    fn golden_manifest_json() {
        let json = serde_json::to_string_pretty(&sample()).unwrap();
        insta::assert_snapshot!("golden_manifest_json", json);
    }
}
