//! Linux (Linux/WSL) `Installer` implementation: download → verify → extract+merge
//! → place (lib64 fix + atomic rename) → smoke test. `scan`/`adopt` stay in-place
//! (M1, ADR-005) and only delegate to [`crate::unix::adopt`].

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cuvm_app::{AcquirePlan, ArtifactKind, Cached, Installer};
use cuvm_core::{Bundle, Candidate, Platform, VersionMeta};
use cuvm_download::{extract_tar_xz, sha256_file, strip_wrapper_dir, Downloader, Reporter};

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

/// Recursively copy every entry from `src` into `dst`, creating directories as
/// needed. Redist component trees are disjoint (one ships `bin/`, another `lib/`),
/// so a later file overwriting an earlier one is not expected; we overwrite rather
/// than error to stay idempotent on re-extract.
fn merge_tree(src: &Path, dst: &Path) -> Result<()> {
    for entry in
        std::fs::read_dir(src).with_context(|| format!("reading staging dir {}", src.display()))?
    {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            std::fs::create_dir_all(&to).with_context(|| format!("mkdir {}", to.display()))?;
            merge_tree(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("mkdir {}", parent.display()))?;
            }
            std::fs::copy(&from, &to)
                .with_context(|| format!("copy {} -> {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Create the relative `lib64 -> lib` symlink at `lib64`. Relative (not absolute)
/// so the tree stays relocatable after the atomic rename to `versions/<ver>`.
#[cfg(unix)]
fn symlink_lib64(lib64: &Path) -> Result<()> {
    std::os::unix::fs::symlink("lib", lib64)
        .with_context(|| format!("creating lib64 -> lib symlink at {}", lib64.display()))
}

/// Non-unix stub: the `UnixInstaller` is only constructed on unix, but this keeps
/// the crate compiling on a windows host build of the workspace.
#[cfg(not(unix))]
fn symlink_lib64(_lib64: &Path) -> Result<()> {
    anyhow::bail!("lib64 symlink is only supported on unix targets")
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
    /// Progress sink (defaults to silent; the CLI injects an indicatif reporter).
    pub(crate) reporter: Reporter,
}

impl UnixInstaller {
    /// Production constructor: scans `/usr/local`, caches under `<temp>/cuvm-cache`.
    #[must_use]
    pub fn new(platform: Platform) -> Self {
        Self {
            scan_root: PathBuf::from("/usr/local"),
            cache_dir: std::env::temp_dir().join("cuvm-cache"),
            platform,
            reporter: cuvm_download::silent(),
        }
    }

    /// Test/override constructor: scans an arbitrary root (e.g. an `assert_fs` tree).
    #[must_use]
    pub fn with_scan_root(scan_root: PathBuf, platform: Platform) -> Self {
        Self {
            scan_root,
            cache_dir: std::env::temp_dir().join("cuvm-cache"),
            platform,
            reporter: cuvm_download::silent(),
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
            reporter: cuvm_download::silent(),
        }
    }

    /// Inject a progress reporter (the composition root supplies an indicatif one).
    #[must_use]
    pub fn with_reporter(mut self, reporter: Reporter) -> Self {
        self.reporter = reporter;
        self
    }
}

impl Installer for UnixInstaller {
    fn acquire(&self, plan: &AcquirePlan) -> Result<Vec<Cached>> {
        std::fs::create_dir_all(&self.cache_dir)
            .with_context(|| format!("creating download cache dir {}", self.cache_dir.display()))?;
        let downloader = Downloader::with_reporter(self.cache_dir.clone(), self.reporter.clone());
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
        self.reporter.on_phase("Verifying");
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
    fn extract_atomic(&self, arts: &[Cached], tmp: &Path) -> Result<PathBuf> {
        self.reporter.on_phase("Extracting");
        // Start from a clean tmp prefix so a re-run never merges stale files.
        if tmp.exists() {
            std::fs::remove_dir_all(tmp)
                .with_context(|| format!("clearing stale temp tree {}", tmp.display()))?;
        }
        std::fs::create_dir_all(tmp)
            .with_context(|| format!("creating temp tree {}", tmp.display()))?;

        for (idx, cached) in arts.iter().enumerate() {
            // Extract each archive into its own staging dir, strip the one wrapper
            // level, then merge the stripped contents into the shared `tmp` prefix.
            let staging = tmp
                .parent()
                .unwrap_or(tmp)
                .join(format!(".stage-{idx}-{}", cached.artifact.component));
            if staging.exists() {
                std::fs::remove_dir_all(&staging)
                    .with_context(|| format!("clearing staging {}", staging.display()))?;
            }
            std::fs::create_dir_all(&staging)
                .with_context(|| format!("creating staging {}", staging.display()))?;

            extract_tar_xz(&cached.path, &staging).with_context(|| {
                format!(
                    "extracting {} ({})",
                    cached.artifact.component,
                    cached.path.display()
                )
            })?;
            strip_wrapper_dir(&staging).with_context(|| {
                format!("stripping wrapper dir for {}", cached.artifact.component)
            })?;
            merge_tree(&staging, tmp).with_context(|| {
                format!(
                    "merging {} into {}",
                    cached.artifact.component,
                    tmp.display()
                )
            })?;
            std::fs::remove_dir_all(&staging).ok();
        }
        Ok(tmp.to_path_buf())
    }
    fn place(&self, tmp: &Path, dst: &Path, meta: &VersionMeta) -> Result<()> {
        // MANDATORY lib64 -> lib symlink: redist ships lib/, but nvcc.profile links
        // -L$(TOP)/lib64; without it, linking fails `cannot find -lcudart`.
        let lib = tmp.join("lib");
        let lib64 = tmp.join("lib64");
        if lib.is_dir() && !lib64.exists() {
            symlink_lib64(&lib64)?;
        }

        // Write the .cuvm-meta.json sidecar INTO the staged tree, so the atomic
        // rename publishes the metadata together with the toolkit (never-partial).
        let meta_json =
            serde_json::to_string_pretty(meta).context("serializing .cuvm-meta.json")?;
        std::fs::write(tmp.join(".cuvm-meta.json"), meta_json)
            .with_context(|| format!("writing {}/.cuvm-meta.json", tmp.display()))?;

        // Atomic publish: a single rename within the same filesystem. Either
        // versions/<ver> appears complete or it never appears at all.
        if dst.exists() {
            std::fs::remove_dir_all(dst)
                .with_context(|| format!("removing existing destination {}", dst.display()))?;
        }
        std::fs::rename(tmp, dst)
            .with_context(|| format!("atomic rename {} -> {}", tmp.display(), dst.display()))?;
        Ok(())
    }
    fn smoke_test(&self, root: &Path) -> Result<()> {
        let nvcc = root.join("bin").join("nvcc");
        if !nvcc.is_file() {
            anyhow::bail!(
                "smoke test: nvcc not found at {} (install is missing bin/nvcc)",
                nvcc.display()
            );
        }

        // Tiny program that pulls in the cudart runtime so linking must resolve
        // -lcudart through <root>/lib64 (catches the missing lib64 symlink) and
        // exercises the external host gcc/g++ that nvcc drives.
        let scratch = tempfile::tempdir().context("creating smoke-test scratch dir")?;
        let src = scratch.path().join("cuvm_smoke.cu");
        let out = scratch.path().join("cuvm_smoke");
        std::fs::write(
            &src,
            "#include <cuda_runtime.h>\n\
             int main() { int n = 0; cudaGetDeviceCount(&n); return 0; }\n",
        )
        .context("writing smoke-test source")?;

        let lib64 = root.join("lib64");
        let output = std::process::Command::new(&nvcc)
            .arg(&src)
            .arg("-o")
            .arg(&out)
            .arg(format!("-L{}", lib64.display()))
            .arg("-lcudart")
            .output()
            .with_context(|| format!("running {}", nvcc.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "smoke test compile+link failed (nvcc exit {}):\n{}\n\
                 hint: an incompatible host gcc/g++ is the usual cause — retry with \
                 `--allow-unsupported-compiler` or point nvcc at a supported compiler via `-ccbin <path>`.",
                output.status.code().unwrap_or(-1),
                stderr.trim_end()
            );
        }
        Ok(())
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
