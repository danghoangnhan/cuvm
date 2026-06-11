//! Content-addressed cuDNN store (`$CUVM_HOME/cudnn/<sha256>/`) and the
//! per-toolkit `.cuvm-cudnn.json` sidecar (spec §2.3, §6, §10; plan D6).

use std::path::{Path, PathBuf};

use cuvm_core::CudnnRecord;

use crate::atomic::write_atomic;
use crate::error::Result;
use crate::layout::Layout;

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

/// `$CUVM_HOME/cudnn/<sha256>` — one immutable payload per content hash.
#[must_use]
pub fn store_path(layout: &Layout, sha256: &str) -> PathBuf {
    layout.cudnn_dir().join(sha256)
}

/// Atomically publish a staged, wrapper-stripped cuDNN tree into the
/// content-addressed store. Idempotent: an existing payload for the same hash
/// wins and the duplicate staging dir is removed (content-addressed ⇒ same
/// bytes). Same never-partial posture as toolkit `place`: the atomicity rests
/// on `rename` staying within one filesystem, and that is the caller's
/// obligation — staging dirs must live under `cudnn/`.
///
/// # Errors
/// [`crate::StoreError::Io`] on filesystem failures.
pub fn place_staged(layout: &Layout, sha256: &str, staged: &Path) -> Result<PathBuf> {
    use crate::error::StoreError;
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source: std::io::Error| StoreError::Io { path, source }
    };

    let dst = store_path(layout, sha256);
    if dst.is_dir() {
        std::fs::remove_dir_all(staged).map_err(io(staged))?;
        return Ok(dst);
    }
    std::fs::create_dir_all(layout.cudnn_dir()).map_err(io(&layout.cudnn_dir()))?;
    if let Err(err) = std::fs::rename(staged, &dst) {
        // Lost a benign race: a concurrent placement published the same
        // content-addressed payload between our existence check and the
        // rename. Treat it as the idempotent-success path above.
        if dst.is_dir() {
            std::fs::remove_dir_all(staged).map_err(io(staged))?;
            return Ok(dst);
        }
        return Err(io(&dst)(err));
    }
    Ok(dst)
}

/// File names of the cuDNN payload's linkable artifacts (`lib/` + `bin/`
/// entries whose name contains `cudnn`), sorted — recorded as `Cudnn.libs`
/// ("full `libcudnn*` set", spec §2.3).
#[must_use]
pub fn lib_names(store: &Path) -> Vec<String> {
    let mut names: Vec<String> = ["lib", "bin"]
        .iter()
        .filter_map(|sub| std::fs::read_dir(store.join(sub)).ok())
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.contains("cudnn"))
        .collect();
    names.sort();
    names.dedup();
    names
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

    fn staged_tree(root: &Path, names: &[&str]) -> PathBuf {
        let staged = root.join(".stage-test");
        for n in names {
            let p = staged.join(n);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        staged
    }

    #[test]
    fn place_staged_moves_the_tree_under_its_sha() {
        let home = tempfile::tempdir().unwrap();
        let layout = Layout::new(home.path());
        let staged = staged_tree(home.path(), &["lib/libcudnn.so", "include/cudnn.h"]);
        let dst = place_staged(&layout, "feedbeef", &staged).unwrap();
        assert_eq!(dst, store_path(&layout, "feedbeef"));
        assert!(dst.join("lib/libcudnn.so").is_file());
        assert!(!staged.exists(), "staging dir is consumed");
    }

    #[test]
    fn place_staged_is_idempotent_for_an_existing_payload() {
        let home = tempfile::tempdir().unwrap();
        let layout = Layout::new(home.path());
        let first = staged_tree(home.path(), &["lib/libcudnn.so"]);
        let dst = place_staged(&layout, "feedbeef", &first).unwrap();
        std::fs::write(dst.join("marker"), b"original").unwrap();
        // Second placement of the same content hash: keep the existing payload.
        let second = staged_tree(home.path(), &["lib/libcudnn.so"]);
        let again = place_staged(&layout, "feedbeef", &second).unwrap();
        assert_eq!(again, dst);
        assert!(dst.join("marker").is_file(), "existing payload untouched");
        assert!(!second.exists(), "duplicate staging cleaned up");
    }

    #[test]
    fn lib_names_collects_cudnn_entries_sorted_and_deduped() {
        let home = tempfile::tempdir().unwrap();
        let store = home.path().join("s");
        for f in [
            "lib/libcudnn_ops.so",
            "lib/libcudnn.so",
            "lib/README",
            "bin/cudnn64_9.dll",
            // Same name in both lib/ and bin/: recorded once.
            "bin/libcudnn.so",
        ] {
            let p = store.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        assert_eq!(
            lib_names(&store),
            ["cudnn64_9.dll", "libcudnn.so", "libcudnn_ops.so"]
        );
    }

    #[test]
    fn lib_names_handles_a_lib_only_tree() {
        // The universal Linux shape: lib/ only, no bin/ — the missing
        // directory is skipped, not an error.
        let home = tempfile::tempdir().unwrap();
        let store = home.path().join("s");
        std::fs::create_dir_all(store.join("lib")).unwrap();
        std::fs::write(store.join("lib/libcudnn.so.9"), b"x").unwrap();
        assert_eq!(lib_names(&store), ["libcudnn.so.9"]);
    }
}
