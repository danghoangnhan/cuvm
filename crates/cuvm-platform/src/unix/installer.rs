//! Linux (Linux/WSL) `Installer` implementation: download → verify → extract+merge
//! → place (lib64 fix + atomic rename) → smoke test. `scan`/`adopt` stay in-place
//! (M1, ADR-005) and only delegate to [`crate::unix::adopt`].

use std::path::{Path, PathBuf};

use anyhow::Result;
use cuvm_app::{AcquirePlan, ArtifactKind, Cached, Installer};
use cuvm_core::{Bundle, Candidate, Platform, VersionMeta};

use crate::not_impl;
use crate::unix::adopt;

/// Unix (Linux/WSL) implementation of the `Installer` port.
pub struct UnixInstaller {
    /// Directory under which `cuda-X.Y` dirs (+ the `cuda` symlink) are sought.
    /// Production default is `/usr/local`; tests inject a fixture root.
    pub(crate) scan_root: PathBuf,
    /// Cache directory for downloaded artifacts (`.part` resume + final files).
    /// Production default is `<temp>/cuvm-cache`; tests inject a `tempfile` dir.
    // Read by `acquire` (Task 13.2); until then no lib-side reader exists.
    #[allow(dead_code)]
    pub(crate) cache_dir: PathBuf,
    /// Host platform recorded on adopted candidates.
    pub(crate) platform: Platform,
}

impl UnixInstaller {
    /// Production constructor: scans `/usr/local`, caches under `<temp>/cuvm-cache`.
    #[must_use]
    pub fn new(platform: Platform) -> Self {
        Self {
            scan_root: PathBuf::from("/usr/local"),
            cache_dir: std::env::temp_dir().join("cuvm-cache"),
            platform,
        }
    }

    /// Test/override constructor: scans an arbitrary root (e.g. an `assert_fs` tree).
    #[must_use]
    pub fn with_scan_root(scan_root: PathBuf, platform: Platform) -> Self {
        Self {
            scan_root,
            cache_dir: std::env::temp_dir().join("cuvm-cache"),
            platform,
        }
    }

    /// Test/override constructor: inject the artifact cache directory.
    ///
    /// The composition root (WU-15) uses this to point the installer at
    /// `<CUVM_HOME>/cache`; tests point it at a `tempfile` dir.
    #[must_use]
    pub fn with_cache_dir(cache_dir: PathBuf, platform: Platform) -> Self {
        Self {
            scan_root: PathBuf::from("/usr/local"),
            cache_dir,
            platform,
        }
    }
}

impl Installer for UnixInstaller {
    fn acquire(&self, _plan: &AcquirePlan) -> Result<Vec<Cached>> {
        Err(not_impl("UnixInstaller::acquire"))
    }
    fn verify(&self, _arts: &[Cached]) -> Result<()> {
        Err(not_impl("UnixInstaller::verify"))
    }
    fn extract_atomic(&self, _arts: &[Cached], _tmp: &Path) -> Result<PathBuf> {
        Err(not_impl("UnixInstaller::extract_atomic"))
    }
    fn place(&self, _tmp: &Path, _dst: &Path, _meta: &VersionMeta) -> Result<()> {
        Err(not_impl("UnixInstaller::place"))
    }
    fn smoke_test(&self, _root: &Path) -> Result<()> {
        Err(not_impl("UnixInstaller::smoke_test"))
    }
    fn ingest_supplied(&self, _file: &Path, _kind: ArtifactKind) -> Result<PathBuf> {
        Err(not_impl("UnixInstaller::ingest_supplied"))
    }
    fn scan(&self) -> Result<Vec<Candidate>> {
        Ok(adopt::scan_root(&self.scan_root, self.platform))
    }
    fn adopt(&self, c: &Candidate) -> Result<Bundle> {
        adopt::adopt_candidate(c)
    }
}

#[cfg(test)]
mod wiring_tests {
    use super::UnixInstaller;
    use cuvm_core::{Arch, Os, Platform};

    #[test]
    fn installer_is_constructible_and_download_dep_links() {
        // Touch the cuvm-download surface so a missing Cargo dep fails to compile.
        let marker: fn(std::path::PathBuf) -> cuvm_download::Downloader =
            cuvm_download::Downloader::new;
        let _downloader = marker(std::env::temp_dir().join("cuvm-wiring-cache"));
        let platform = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        let i = UnixInstaller::new(platform);
        // Touch the injected cache dir field so the install pipeline wiring is live.
        assert!(i.cache_dir.ends_with("cuvm-cache"));
    }
}
