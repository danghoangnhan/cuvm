//! `$CUVM_HOME` resolution and well-known on-disk paths.

use std::path::{Path, PathBuf};

use crate::error::{Result, StoreError};

/// Resolved on-disk layout rooted at `$CUVM_HOME`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    /// Construct from an already-known home root.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Layout { root: root.into() }
    }

    /// Resolve using an injected env getter and an injected home-dir fallback.
    /// `get_env("CUVM_HOME")` wins; else `<home_dir>/.cuvm`.
    ///
    /// # Errors
    /// Returns [`StoreError::HomeUnresolved`] if no `CUVM_HOME` is set and
    /// `home_dir` is `None`.
    pub fn resolve_with<F>(get_env: F, home_dir: Option<PathBuf>) -> Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        if let Some(explicit) = get_env("CUVM_HOME") {
            if !explicit.trim().is_empty() {
                return Ok(Layout::new(PathBuf::from(explicit)));
            }
        }
        let home = home_dir.ok_or_else(|| {
            StoreError::HomeUnresolved("no CUVM_HOME and no home directory available".to_string())
        })?;
        Ok(Layout::new(home.join(".cuvm")))
    }

    /// Root of the `CUVM_HOME` directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path to the manifest: `$CUVM_HOME/manifest.json`.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    /// Path to the versions directory: `$CUVM_HOME/versions`.
    #[must_use]
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// Path to a specific version directory: `$CUVM_HOME/versions/<ver>`.
    #[must_use]
    pub fn version_dir(&self, ver: &str) -> PathBuf {
        self.versions_dir().join(ver)
    }

    /// Path to the per-version sidecar: `$CUVM_HOME/versions/<ver>/.cuvm-meta.json`.
    #[must_use]
    pub fn meta_path(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join(".cuvm-meta.json")
    }

    /// Path to the cuDNN content store: `$CUVM_HOME/cudnn`.
    #[must_use]
    pub fn cudnn_dir(&self) -> PathBuf {
        self.root.join("cudnn")
    }

    /// Path to the NCCL content store: `$CUVM_HOME/nccl`.
    #[must_use]
    pub fn nccl_dir(&self) -> PathBuf {
        self.root.join("nccl")
    }

    /// Path to the recorded EULA acknowledgements (spec §6: `eula/`).
    #[must_use]
    pub fn eula_dir(&self) -> PathBuf {
        self.root.join("eula")
    }

    /// Resolve a manifest `path` field: absolute (adopted) returned as-is;
    /// relative (`versions/<ver>`) joined against the home root.
    #[must_use]
    pub fn resolve_record_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.root.join(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuvm_home_env_override_wins() {
        let l = Layout::resolve_with(
            |k| (k == "CUVM_HOME").then(|| "/custom/cuvmhome".to_string()),
            Some(PathBuf::from("/home/u")),
        )
        .unwrap();
        assert_eq!(l.root(), Path::new("/custom/cuvmhome"));
        assert_eq!(
            l.manifest_path(),
            Path::new("/custom/cuvmhome/manifest.json")
        );
    }

    #[test]
    fn falls_back_to_home_dot_cuvm() {
        let l = Layout::resolve_with(|_| None, Some(PathBuf::from("/home/u"))).unwrap();
        assert_eq!(l.root(), Path::new("/home/u/.cuvm"));
        assert_eq!(
            l.meta_path("12.4.1"),
            Path::new("/home/u/.cuvm/versions/12.4.1/.cuvm-meta.json")
        );
    }

    #[test]
    fn empty_cuvm_home_is_ignored_and_falls_back() {
        let l = Layout::resolve_with(
            |k| (k == "CUVM_HOME").then(|| "   ".to_string()),
            Some(PathBuf::from("/home/u")),
        )
        .unwrap();
        assert_eq!(l.root(), Path::new("/home/u/.cuvm"));
    }

    #[test]
    fn no_home_no_env_is_typed_error_not_panic() {
        let err = Layout::resolve_with(|_| None, None).unwrap_err();
        assert!(matches!(err, StoreError::HomeUnresolved(_)));
    }

    #[test]
    fn adopted_absolute_path_kept_relative_path_joined() {
        let l = Layout::new("/home/u/.cuvm");
        assert_eq!(
            l.resolve_record_path("/usr/local/cuda-12.4"),
            Path::new("/usr/local/cuda-12.4")
        );
        assert_eq!(
            l.resolve_record_path("versions/12.4.1"),
            Path::new("/home/u/.cuvm/versions/12.4.1")
        );
    }
}
