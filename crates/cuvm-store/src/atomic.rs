//! Atomic file replacement: write temp in the same dir, fsync, rename over target.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::error::{Result, StoreError};

/// Atomically write `bytes` to `target`. Creates parent dirs. On success the
/// temp file is gone and `target` is the new content; on a write failure the
/// temp is cleaned up and `target` is left untouched.
///
/// # Errors
/// Returns [`StoreError::Io`] if creating parent directories, writing the temp
/// file, or renaming it over the target fails.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| StoreError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tmp = temp_sibling(target);
    let res = (|| -> std::io::Result<()> {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(source) = res {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::Io { path: tmp, source });
    }
    fs::rename(&tmp, target).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        StoreError::Io {
            path: target.to_path_buf(),
            source,
        }
    })?;
    Ok(())
}

fn temp_sibling(target: &Path) -> std::path::PathBuf {
    let pid = std::process::id();
    let name = target
        .file_name()
        .map_or_else(|| "cuvm".to_string(), |n| n.to_string_lossy().into_owned());
    let tmp_name = format!(".{name}.tmp.{pid}");
    match target.parent() {
        Some(p) => p.join(tmp_name),
        None => std::path::PathBuf::from(tmp_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_content_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("manifest.json");
        write_atomic(&target, b"{\"schema_version\":1}").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"{\"schema_version\":1}");
        // no leftover temp siblings
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "found temp leftovers: {leftovers:?}");
    }

    #[test]
    fn overwrites_existing_target_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("manifest.json");
        write_atomic(&target, b"old").unwrap();
        write_atomic(&target, b"new-and-longer").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new-and-longer");
    }

    #[test]
    fn creates_missing_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("versions/12.4.1/.cuvm-meta.json");
        write_atomic(&target, b"{}").unwrap();
        assert!(target.exists());
    }

    #[test]
    fn original_intact_when_rename_target_is_a_directory() {
        // Simulated failure: target path is an existing directory, so rename
        // over it fails; the pre-existing sibling file must remain untouched.
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("keep.json");
        write_atomic(&good, b"precious").unwrap();
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        let err = write_atomic(&blocked, b"junk").unwrap_err();
        assert!(matches!(err, StoreError::Io { .. }));
        // unrelated file survived and no temp leaked
        assert_eq!(fs::read(&good).unwrap(), b"precious");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "temp leaked: {leftovers:?}");
    }
}
