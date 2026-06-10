//! Content-addressed cuDNN store (`$CUVM_HOME/cudnn/<sha256>/`) and the
//! per-toolkit `.cuvm-cudnn.json` sidecar (spec §2.3, §6, §10; plan D6).

use std::path::{Path, PathBuf};

use cuvm_core::CudnnRecord;

use crate::atomic::write_atomic;
use crate::error::Result;

/// `versions/<ver>/.cuvm-cudnn.json` for a placed toolkit root.
#[must_use]
pub fn cudnn_meta_path(toolkit_root: &Path) -> PathBuf {
    toolkit_root.join(".cuvm-cudnn.json")
}

/// Read the sidecar; `None` on missing/corrupt (hydration must never error —
/// same posture as the redist cache).
#[must_use]
pub fn read_cudnn_meta(toolkit_root: &Path) -> Option<CudnnRecord> {
    let bytes = std::fs::read(cudnn_meta_path(toolkit_root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Atomically write the sidecar.
///
/// # Errors
/// Returns [`crate::error::StoreError::Io`] when the write fails.
///
/// # Panics
/// Panics only if the (infallible) [`CudnnRecord`] serialization fails, which
/// cannot happen for this all-owned, plain-data document.
pub fn write_cudnn_meta(toolkit_root: &Path, rec: &CudnnRecord) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(rec).expect("CudnnRecord serializes");
    write_atomic(&cudnn_meta_path(toolkit_root), &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::Source;

    fn rec() -> CudnnRecord {
        CudnnRecord {
            version: "9.8.0".into(),
            cuda_major: 12,
            source: Source::Downloaded,
            sha256: "feedbeef".into(),
            libs: vec!["libcudnn.so".into()],
            installed_at: time::macros::datetime!(2026-06-10 10:30:00 UTC),
        }
    }

    #[test]
    fn sidecar_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        write_cudnn_meta(dir.path(), &rec()).unwrap();
        assert_eq!(read_cudnn_meta(dir.path()), Some(rec()));
    }

    #[test]
    fn missing_or_corrupt_sidecar_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_cudnn_meta(dir.path()), None);
        std::fs::write(cudnn_meta_path(dir.path()), b"{not json").unwrap();
        assert_eq!(read_cudnn_meta(dir.path()), None);
    }
}
