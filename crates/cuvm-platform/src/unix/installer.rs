//! Linux (Linux/WSL) `Installer` implementation: download → verify → extract+merge
//! → place (lib64 fix + atomic rename) → smoke test. `scan`/`adopt` stay in-place
//! (M1, ADR-005) and only delegate to [`crate::unix::adopt`].

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cuvm_app::{AcquirePlan, ArtifactKind, Cached, Installer};
use cuvm_core::{Bundle, Candidate, Platform, VersionMeta};
use cuvm_download::{sha256_file, Downloader};

use crate::not_impl;
use crate::unix::adopt;

/// The on-disk cache file name for an artifact: the final segment of its
/// `relative_path` (e.g. `cuda_cudart-linux-x86_64-12.4.131-archive.tar.xz`).
///
/// Redist relative paths are unique per component+platform+version, so the last
/// segment never collides across the artifacts of one toolkit.
pub(crate) fn artifact_file_name(a: &cuvm_app::Artifact) -> String {
    a.relative_path
        .rsplit('/')
        .next()
        .unwrap_or(&a.relative_path)
        .to_string()
}

/// Unix (Linux/WSL) implementation of the `Installer` port.
pub struct UnixInstaller {
    /// Directory under which `cuda-X.Y` dirs (+ the `cuda` symlink) are sought.
    /// Production default is `/usr/local`; tests inject a fixture root.
    pub(crate) scan_root: PathBuf,
    /// Cache directory for downloaded artifacts (`.part` resume + final files).
    /// Production default is `<temp>/cuvm-cache`; tests inject a `tempfile` dir.
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
    fn acquire(&self, plan: &AcquirePlan) -> Result<Vec<Cached>> {
        std::fs::create_dir_all(&self.cache_dir)
            .with_context(|| format!("creating download cache dir {}", self.cache_dir.display()))?;
        let downloader = Downloader::new(self.cache_dir.clone());
        let mut out = Vec::with_capacity(plan.artifacts.len());
        for artifact in &plan.artifacts {
            let file_name = artifact_file_name(artifact);
            let path = downloader
                .fetch(&artifact.url, &artifact.sha256, &file_name)
                .with_context(|| {
                    format!(
                        "acquiring component {} from {}",
                        artifact.component, artifact.url
                    )
                })?;
            out.push(Cached {
                artifact: artifact.clone(),
                path,
            });
        }
        Ok(out)
    }

    fn verify(&self, arts: &[Cached]) -> Result<()> {
        for cached in arts {
            let got = sha256_file(&cached.path)
                .with_context(|| format!("hashing cached artifact {}", cached.path.display()))?;
            if got != cached.artifact.sha256 {
                anyhow::bail!(
                    "sha256 mismatch for {} ({}): expected {}, got {}",
                    cached.artifact.component,
                    cached.path.display(),
                    cached.artifact.sha256,
                    got
                );
            }
        }
        Ok(())
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

#[cfg(test)]
mod acquire_verify_tests {
    use super::{artifact_file_name, UnixInstaller};
    use cuvm_app::{Artifact, Cached, Installer};
    use cuvm_core::{Arch, Os, Platform};
    use std::io::Write;

    fn art(relative_path: &str, sha256: &str) -> Artifact {
        Artifact {
            component: "cuda_cudart".into(),
            relative_path: relative_path.into(),
            url: format!("https://example.test/{relative_path}"),
            sha256: sha256.into(),
            md5: None,
            size: 0,
        }
    }

    #[test]
    fn file_name_is_the_last_relative_path_segment() {
        let a = art(
            "cuda_cudart/linux-x86_64/cuda_cudart-linux-x86_64-12.4.131-archive.tar.xz",
            "00",
        );
        assert_eq!(
            artifact_file_name(&a),
            "cuda_cudart-linux-x86_64-12.4.131-archive.tar.xz"
        );
    }

    #[test]
    fn verify_passes_when_sha256_matches_on_disk_bytes() {
        // sha256("hello\n") is well known.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cuda_cudart-archive.tar.xz");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"hello\n").unwrap();
        drop(f);
        let sha = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";
        let cached = vec![Cached {
            artifact: art("c/x.tar.xz", sha),
            path,
        }];

        let platform = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        let i = UnixInstaller::new(platform);
        i.verify(&cached).expect("matching sha256 verifies");
    }

    #[test]
    fn verify_errors_on_sha256_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cuda_cudart-archive.tar.xz");
        std::fs::write(&path, b"hello\n").unwrap();
        let cached = vec![Cached {
            artifact: art(
                "c/x.tar.xz",
                "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            ),
            path,
        }];

        let platform = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        let i = UnixInstaller::new(platform);
        let err = i.verify(&cached).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("sha256") && msg.contains("mismatch"), "{msg}");
    }
}
