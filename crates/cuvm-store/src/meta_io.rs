//! Read/write the per-version `.cuvm-meta.json` sidecar.

use std::fs;
use std::path::Path;

use cuvm_core::VersionMeta;

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};

/// Read the per-version sidecar. Missing file is a typed I/O error (not silent).
///
/// # Errors
/// Returns [`StoreError::Io`] if the file cannot be read, or
/// [`StoreError::Corrupt`] if the file contains invalid JSON.
pub fn read_meta(path: &Path) -> Result<VersionMeta> {
    let bytes = fs::read(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| StoreError::Corrupt {
        path: path.to_path_buf(),
        source,
    })
}

/// Serialize pretty + atomically replace the sidecar file.
///
/// # Errors
/// Returns [`StoreError::Io`] if the atomic write fails.
///
/// # Panics
/// Never panics — `VersionMeta` is always serializable.
pub fn write_meta(path: &Path, meta: &VersionMeta) -> Result<()> {
    let json = serde_json::to_vec_pretty(meta).expect("VersionMeta is always serializable");
    write_atomic(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::Source;
    use time::macros::datetime;

    fn sample() -> VersionMeta {
        VersionMeta {
            version: "12.4.1".to_string(),
            source: Source::Downloaded,
            cudnn: Some("9.7.0".to_string()),
            components: vec!["cuda_nvcc".to_string()],
            sha256: Some("abc".to_string()),
            has_lib64: false,
            installed_at: datetime!(2026-06-08 10:30:00 UTC),
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("versions/12.4.1/.cuvm-meta.json");
        write_meta(&path, &sample()).unwrap();
        assert_eq!(read_meta(&path).unwrap(), sample());
    }

    #[test]
    fn missing_meta_is_typed_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope/.cuvm-meta.json");
        let err = read_meta(&path).unwrap_err();
        assert!(matches!(err, StoreError::Io { .. }));
    }

    #[test]
    fn corrupt_meta_is_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".cuvm-meta.json");
        fs::write(&path, b"not json").unwrap();
        let err = read_meta(&path).unwrap_err();
        assert!(matches!(err, StoreError::Corrupt { .. }));
    }
}
