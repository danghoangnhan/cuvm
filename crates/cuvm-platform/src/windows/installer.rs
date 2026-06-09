//! Windows `Installer`: read-only scan + in-place adopt of CUDA toolkit trees.
//! Scan roots are injectable so the filesystem walk runs on any host against a
//! fixture tree; production reads `CUDA_PATH`/`CUDA_PATH_V*`/Program Files (§2.2).
//! Download/extract/place land in WU-14 (kept as `not_impl` stubs here).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
#[cfg(test)]
use cuvm_app::Artifact;
use cuvm_app::{AcquirePlan, ArtifactKind, Cached, Candidate, Installer};
use cuvm_core::{Arch, Bundle, Os, Platform, Source, Toolkit, Version, VersionMeta};
use cuvm_download::Reporter;
use time::OffsetDateTime;

/// Outcome of attempting the Windows download/assemble path. The CLI (WU-15)
/// matches on this: `Assembled` continues to `extract_atomic`/`place`, while
/// `DegradeToAdopt` makes `install` fall back to the read-only adopt pipeline
/// (spec §2.2: auto-degrade to adopt-only when enterprise lockdown blocks the
/// download, or when no `windows-x86_64` components exist — N/A from CUDA 13.0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsAcquireOutcome {
    /// All `windows-x86_64` `.zip` components were downloaded into the cache.
    Assembled(Vec<Cached>),
    /// No usable download path; the installer should adopt an existing tree.
    DegradeToAdopt {
        /// Human-readable reason the install degraded (logged by WU-15).
        reason: String,
    },
}

/// Windows installer over scan roots (Program Files + `CUDA_PATH*`) plus an
/// injectable download cache and a destination base (`%USERPROFILE%\.cuvm\versions`).
pub struct WindowsInstaller {
    roots: Vec<PathBuf>,
    cache_dir: PathBuf,
    dest_base: PathBuf,
    reporter: Reporter,
}

impl Default for WindowsInstaller {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsInstaller {
    /// Construct with the default Windows scan roots + a default cache/dest under
    /// `%USERPROFILE%\.cuvm` (read from the environment; empty/neutral on Linux CI).
    #[must_use]
    pub fn new() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let base = home.join(".cuvm");
        WindowsInstaller {
            roots: default_roots(),
            cache_dir: base.join("cache"),
            dest_base: base.join("versions"),
            reporter: cuvm_download::silent(),
        }
    }

    /// Construct over explicit scan roots (used by WU-9 scan/adopt tests).
    #[must_use]
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        let mut s = Self::new();
        s.roots = roots;
        s
    }

    /// Test/override constructor: inject the download cache, destination base, and
    /// scan roots so the whole pipeline runs against fixtures on any host.
    #[must_use]
    pub fn with_paths(cache_dir: PathBuf, dest_base: PathBuf, roots: Vec<PathBuf>) -> Self {
        WindowsInstaller {
            roots,
            cache_dir,
            dest_base,
            reporter: cuvm_download::silent(),
        }
    }

    /// Inject a progress reporter (the composition root supplies an indicatif one).
    #[must_use]
    pub fn with_reporter(mut self, reporter: Reporter) -> Self {
        self.reporter = reporter;
        self
    }

    fn windows_platform() -> Platform {
        Platform {
            os: Os::Windows,
            arch: Arch::X86_64,
        }
    }

    /// The destination base under which `versions/<handle>` trees are placed
    /// (`%USERPROFILE%\.cuvm\versions`). The composition root (WU-15) reads this to
    /// derive the per-handle `dst`/`.tmp-<handle>` paths it passes to
    /// `extract_atomic`/`place`, so the installer owns where its installs land.
    #[must_use]
    pub fn dest_base(&self) -> &Path {
        &self.dest_base
    }

    /// Pure degrade decision over a resolved plan. An empty `windows-x86_64`
    /// component set (registry miss, or CUDA >= 13.0 where Windows is N/A) means
    /// there is nothing to download → adopt-only. A non-empty plan is left for the
    /// real `acquire` to download (this returns an empty `Assembled` marker; the
    /// actual `Vec<Cached>` is filled by `acquire`).
    #[must_use]
    pub fn decide_acquire(plan: &AcquirePlan) -> WindowsAcquireOutcome {
        if plan.artifacts.is_empty() {
            return WindowsAcquireOutcome::DegradeToAdopt {
                reason: format!(
                    "no windows-x86_64 redist components resolved for {} (CUDA >= 13.0 is \
                     Windows-N/A, or the registry returned no Windows artifacts)",
                    plan.dest_handle
                ),
            };
        }
        WindowsAcquireOutcome::Assembled(Vec::new())
    }

    /// Classify a download failure (HTTP 403/blocked, proxy/SmartScreen/AppLocker,
    /// connection refused) as a degrade-to-adopt signal so `install` can fall back
    /// rather than hard-fail (spec §2.2: enterprise lockdown auto-degrade).
    #[must_use]
    pub fn degrade_on_download_error(err: &anyhow::Error) -> WindowsAcquireOutcome {
        WindowsAcquireOutcome::DegradeToAdopt {
            reason: format!("windows-x86_64 download blocked, degrading to adopt-only: {err}"),
        }
    }

    /// Recursively merge `src` into `dst`, creating dirs and moving files. Used to
    /// fold each component's flattened tree into the single Windows prefix so
    /// `bin\` and `lib\x64\` from every component coexist (no `lib64` symlink).
    fn merge_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                std::fs::create_dir_all(&to)?;
                Self::merge_tree(&from, &to)?;
            } else {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(&from, &to)?;
            }
        }
        Ok(())
    }
}

/// Default scan roots per spec §2.2: Program Files install dir + `CUDA_PATH` +
/// every `CUDA_PATH_VX_Y`. Reading real env is host-neutral (empty on Linux CI).
fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        roots.push(
            PathBuf::from(pf)
                .join("NVIDIA GPU Computing Toolkit")
                .join("CUDA"),
        );
    }
    for (k, v) in std::env::vars() {
        if k == "CUDA_PATH" || k.starts_with("CUDA_PATH_V") {
            // CUDA_PATH points at a vX.Y dir; scan its parent so the walk finds it.
            if let Some(parent) = PathBuf::from(&v).parent() {
                roots.push(parent.to_path_buf());
            }
        }
    }
    roots
}

/// Parse `"v12.4"` → `Version("12.4")`; `None` for non-version dir names.
fn parse_version_dir(name: &str) -> Option<Version> {
    let stripped = name.strip_prefix('v').or_else(|| name.strip_prefix('V'))?;
    if !stripped.contains('.') {
        return None;
    }
    Version::parse(stripped).ok()
}

impl Installer for WindowsInstaller {
    fn scan(&self) -> Result<Vec<Candidate>> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for root in &self.roots {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue; // root absent => nothing to adopt here
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let Some(version) = parse_version_dir(&name) else {
                    continue;
                };
                let dir = entry.path();
                if !dir.join("bin").join("nvcc.exe").exists() {
                    continue; // not a real toolkit dir
                }
                if seen.insert(dir.clone()) {
                    out.push(Candidate {
                        version,
                        root: dir,
                        platform: Self::windows_platform(),
                        source: Source::Adopted,
                    });
                }
            }
        }
        Ok(out)
    }

    fn adopt(&self, c: &Candidate) -> Result<Bundle> {
        let root = c.root.clone();
        anyhow::ensure!(
            root.join("bin").join("nvcc.exe").exists(),
            "adopt: {} is not a CUDA toolkit (no bin\\nvcc.exe)",
            root.display()
        );
        let toolkit = Toolkit {
            version: c.version.clone(),
            source: Source::Adopted,
            root,
            platform: c.platform,   // Platform is Copy
            components: Vec::new(), // adopted: components unknown, not manifest-driven
            has_lib64: false,       // Windows uses lib\x64; lib64 symlink is Linux-only
            installed_at: OffsetDateTime::now_utc(),
            checksum: None,
        };
        Ok(Bundle {
            toolkit,
            cudnn: None,
            extra: Vec::new(),
        })
    }

    fn acquire(&self, plan: &AcquirePlan) -> Result<Vec<Cached>> {
        use cuvm_download::Downloader;
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
            anyhow::anyhow!(
                "acquire: cannot create cache dir {}: {e}",
                self.cache_dir.display()
            )
        })?;
        let downloader = Downloader::with_reporter(self.cache_dir.clone(), self.reporter.clone());
        let mut out = Vec::with_capacity(plan.artifacts.len());
        for art in &plan.artifacts {
            let file_name = Path::new(&art.relative_path).file_name().map_or_else(
                || art.component.clone(),
                |n| n.to_string_lossy().into_owned(),
            );
            let path = downloader
                .fetch(&art.url, &art.sha256, &file_name)
                .map_err(|e| anyhow::anyhow!("acquire {}: {e}", art.component))?;
            out.push(Cached {
                artifact: art.clone(),
                path,
            });
        }
        Ok(out)
    }

    fn verify(&self, arts: &[Cached]) -> Result<()> {
        self.reporter.on_phase("Verifying");
        for c in arts {
            let got = cuvm_download::sha256_file(&c.path)
                .map_err(|e| anyhow::anyhow!("verify {}: {e}", c.artifact.component))?;
            anyhow::ensure!(
                got.eq_ignore_ascii_case(&c.artifact.sha256),
                "verify {}: sha256 mismatch (expected {}, got {})",
                c.artifact.component,
                c.artifact.sha256,
                got
            );
        }
        Ok(())
    }
    fn extract_atomic(&self, arts: &[Cached], tmp: &Path) -> Result<PathBuf> {
        use cuvm_download::{extract_zip, strip_wrapper_dir};

        self.reporter.on_phase("Extracting");
        // Start from a clean tmp prefix (never-partial: caller renames it into place).
        if tmp.exists() {
            std::fs::remove_dir_all(tmp)
                .map_err(|e| anyhow::anyhow!("extract: clean {} failed: {e}", tmp.display()))?;
        }
        std::fs::create_dir_all(tmp)
            .map_err(|e| anyhow::anyhow!("extract: create {} failed: {e}", tmp.display()))?;

        for (idx, c) in arts.iter().enumerate() {
            // Per-component scratch so we can strip the single wrapper dir before merge.
            let scratch = tmp.join(format!(".extract-{idx}"));
            std::fs::create_dir_all(&scratch).map_err(|e| {
                anyhow::anyhow!("extract: scratch {} failed: {e}", scratch.display())
            })?;
            extract_zip(&c.path, &scratch)
                .map_err(|e| anyhow::anyhow!("extract {}: {e}", c.artifact.component))?;
            // Redist zips wrap in "<comp>-windows-x86_64-<ver>-archive/"; flatten one level.
            strip_wrapper_dir(&scratch)
                .map_err(|e| anyhow::anyhow!("strip wrapper {}: {e}", c.artifact.component))?;
            Self::merge_tree(&scratch, tmp)
                .map_err(|e| anyhow::anyhow!("merge {} into prefix: {e}", c.artifact.component))?;
            std::fs::remove_dir_all(&scratch).ok();
        }
        Ok(tmp.to_path_buf())
    }
    fn place(&self, tmp: &Path, dst: &Path, meta: &VersionMeta) -> Result<()> {
        // Write the sidecar INSIDE the tmp prefix so it lands atomically with the rename.
        let sidecar = tmp.join(".cuvm-meta.json");
        let json = serde_json::to_vec_pretty(meta)
            .map_err(|e| anyhow::anyhow!("place: serialize meta: {e}"))?;
        std::fs::write(&sidecar, &json)
            .map_err(|e| anyhow::anyhow!("place: write {}: {e}", sidecar.display()))?;

        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("place: mkdir {}: {e}", parent.display()))?;
        }
        // Never-partial: drop any stale dst, then a single atomic rename swings the
        // fully-merged tree into place. No junction needed (dst is a real dir move).
        if dst.exists() {
            std::fs::remove_dir_all(dst)
                .map_err(|e| anyhow::anyhow!("place: clear {}: {e}", dst.display()))?;
        }
        std::fs::rename(tmp, dst).map_err(|e| {
            anyhow::anyhow!(
                "place: atomic rename {} -> {} failed: {e}",
                tmp.display(),
                dst.display()
            )
        })?;
        Ok(())
    }

    fn smoke_test(&self, root: &Path) -> Result<()> {
        smoke_test_windows(root)
    }

    fn ingest_supplied(&self, _file: &Path, _kind: ArtifactKind) -> Result<PathBuf> {
        Err(crate::not_impl("WindowsInstaller::ingest_supplied"))
    }
}

/// Windows smoke test: the lighter check from the WU brief — confirm `nvcc.exe`
/// resolves and reports a version. Heavier compile+link is the Linux lane's job.
#[cfg(windows)]
fn smoke_test_windows(root: &Path) -> Result<()> {
    let nvcc = root.join("bin").join("nvcc.exe");
    anyhow::ensure!(nvcc.exists(), "smoke_test: {} not found", nvcc.display());
    let out = std::process::Command::new(&nvcc)
        .arg("--version")
        .output()
        .map_err(|e| anyhow::anyhow!("smoke_test: spawn {} failed: {e}", nvcc.display()))?;
    anyhow::ensure!(
        out.status.success(),
        "smoke_test: nvcc --version exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// Non-Windows hosts cannot run `nvcc.exe`; the install pipeline never reaches a
/// Windows `place` there, so this is a no-op that keeps the code path compiling
/// for `x86_64-pc-windows-gnu` cross-checks and Linux unit tests. The `Result`
/// wrap is mandatory to match the `#[cfg(windows)]` arm + the `smoke_test` caller,
/// so the always-`Ok` shape on this host is intentional.
#[cfg(not(windows))]
#[allow(clippy::unnecessary_wraps)]
fn smoke_test_windows(_root: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod decide_tests {
    use super::*;
    use cuvm_app::AcquirePlan;

    fn art(component: &str) -> Artifact {
        Artifact {
            component: component.into(),
            relative_path: format!(
                "{component}/windows-x86_64/{component}-windows-x86_64-12.4.131-archive.zip"
            ),
            url: format!("https://example.test/{component}.zip"),
            sha256: "00".repeat(32),
            md5: None,
            size: 10,
        }
    }

    #[test]
    fn empty_windows_component_set_degrades_to_adopt() {
        let plan = AcquirePlan {
            artifacts: vec![],
            dest_handle: "12.4".into(),
        };
        match WindowsInstaller::decide_acquire(&plan) {
            WindowsAcquireOutcome::DegradeToAdopt { reason } => {
                assert!(reason.to_lowercase().contains("no windows"), "{reason}");
            }
            other @ WindowsAcquireOutcome::Assembled(_) => {
                panic!("expected degrade, got {other:?}")
            }
        }
    }

    #[test]
    fn non_empty_plan_does_not_degrade_on_decision() {
        let plan = AcquirePlan {
            artifacts: vec![art("cuda_nvcc"), art("cuda_cudart")],
            dest_handle: "12.4".into(),
        };
        assert!(matches!(
            WindowsInstaller::decide_acquire(&plan),
            WindowsAcquireOutcome::Assembled(c) if c.is_empty()
        ));
    }

    #[test]
    fn download_error_is_classified_as_lockdown_degrade() {
        let err = anyhow::anyhow!("HTTP 403 Forbidden (SmartScreen/proxy blocked)");
        let outcome = WindowsInstaller::degrade_on_download_error(&err);
        match outcome {
            WindowsAcquireOutcome::DegradeToAdopt { reason } => {
                assert!(reason.contains("403"), "{reason}");
                assert!(reason.to_lowercase().contains("download"), "{reason}");
            }
            other @ WindowsAcquireOutcome::Assembled(_) => {
                panic!("expected degrade, got {other:?}")
            }
        }
    }
}

#[cfg(test)]
mod verify_tests {
    use super::*;
    use std::io::Write;

    fn make_zip(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file(
            "cuda_nvcc-windows-x86_64-12.4.131-archive/bin/nvcc.exe",
            opts,
        )
        .unwrap();
        zip.write_all(b"MZ placeholder nvcc").unwrap();
        zip.finish().unwrap();
        path
    }

    fn cached(path: std::path::PathBuf, sha256: String) -> Cached {
        Cached {
            artifact: Artifact {
                component: "cuda_nvcc".into(),
                relative_path:
                    "cuda_nvcc/windows-x86_64/cuda_nvcc-windows-x86_64-12.4.131-archive.zip".into(),
                url: "https://example.test/cuda_nvcc.zip".into(),
                sha256,
                md5: None,
                size: 0,
            },
            path,
        }
    }

    #[test]
    fn verify_passes_for_correct_sha256() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = make_zip(tmp.path(), "cuda_nvcc.zip");
        let sha = cuvm_download::sha256_file(&zip).unwrap();
        let inst = WindowsInstaller::with_paths(
            tmp.path().to_path_buf(),
            tmp.path().to_path_buf(),
            vec![],
        );
        inst.verify(&[cached(zip, sha)]).unwrap();
    }

    #[test]
    fn verify_fails_loudly_for_wrong_sha256() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = make_zip(tmp.path(), "cuda_nvcc.zip");
        let inst = WindowsInstaller::with_paths(
            tmp.path().to_path_buf(),
            tmp.path().to_path_buf(),
            vec![],
        );
        let err = inst
            .verify(&[cached(zip, "ff".repeat(32))])
            .unwrap_err()
            .to_string();
        assert!(err.contains("cuda_nvcc"), "{err}");
        assert!(err.to_lowercase().contains("sha256"), "{err}");
    }
}

#[cfg(test)]
mod extract_tests {
    use super::*;
    use std::io::Write;

    fn zip_with(
        dir: &std::path::Path,
        zip_name: &str,
        wrapper: &str,
        inner: &[(&str, &[u8])],
    ) -> Cached {
        let path = dir.join(zip_name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (rel, bytes) in inner {
            zip.start_file(format!("{wrapper}/{rel}"), opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        let sha = cuvm_download::sha256_file(&path).unwrap();
        Cached {
            artifact: Artifact {
                component: zip_name.trim_end_matches(".zip").into(),
                relative_path: format!("c/windows-x86_64/{zip_name}"),
                url: "https://example.test/x.zip".into(),
                sha256: sha,
                md5: None,
                size: 0,
            },
            path,
        }
    }

    #[test]
    fn merges_components_and_strips_wrapper() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let nvcc = zip_with(
            &cache,
            "cuda_nvcc.zip",
            "cuda_nvcc-windows-x86_64-12.4.131-archive",
            &[("bin/nvcc.exe", b"MZ nvcc")],
        );
        let cudart = zip_with(
            &cache,
            "cuda_cudart.zip",
            "cuda_cudart-windows-x86_64-12.4.131-archive",
            &[("lib/x64/cudart64_12.dll", b"MZ cudart")],
        );

        let dest_base = tmp.path().join("versions");
        let inst = WindowsInstaller::with_paths(cache, dest_base, vec![]);
        let scratch = tmp.path().join(".tmp-12.4");
        let merged = inst.extract_atomic(&[nvcc, cudart], &scratch).unwrap();

        assert_eq!(merged, scratch);
        assert!(merged.join("bin").join("nvcc.exe").exists(), "nvcc merged");
        assert!(
            merged
                .join("lib")
                .join("x64")
                .join("cudart64_12.dll")
                .exists(),
            "cudart merged into lib/x64"
        );
        // No leftover wrapper dir, and no Linux lib64 symlink on Windows trees.
        assert!(!merged
            .join("cuda_nvcc-windows-x86_64-12.4.131-archive")
            .exists());
        assert!(!merged.join("lib64").exists(), "windows has no lib64");
    }
}

#[cfg(test)]
mod place_tests {
    use super::*;
    use cuvm_core::Source;
    use time::OffsetDateTime;

    fn meta(version: &str) -> VersionMeta {
        VersionMeta {
            version: version.into(),
            source: Source::Downloaded,
            cudnn: None,
            components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
            sha256: None,
            has_lib64: false,
            installed_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn seed_prefix(tmp: &std::path::Path) {
        std::fs::create_dir_all(tmp.join("bin")).unwrap();
        std::fs::write(tmp.join("bin").join("nvcc.exe"), b"MZ nvcc").unwrap();
        std::fs::create_dir_all(tmp.join("lib").join("x64")).unwrap();
        std::fs::write(tmp.join("lib").join("x64").join("cudart64_12.dll"), b"MZ").unwrap();
    }

    #[test]
    fn place_renames_prefix_and_writes_sidecar() {
        let base = tempfile::tempdir().unwrap();
        let tmp = base.path().join(".tmp-12.4");
        seed_prefix(&tmp);
        let dst = base.path().join("versions").join("12.4");
        let inst = WindowsInstaller::with_paths(
            base.path().join("cache"),
            base.path().join("versions"),
            vec![],
        );

        inst.place(&tmp, &dst, &meta("12.4.131")).unwrap();

        assert!(dst.join("bin").join("nvcc.exe").exists());
        assert!(dst.join("lib").join("x64").join("cudart64_12.dll").exists());
        assert!(!tmp.exists(), "tmp consumed by rename");

        let sidecar = dst.join(".cuvm-meta.json");
        assert!(sidecar.exists(), "sidecar written");
        let on_disk: VersionMeta =
            serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(on_disk.version, "12.4.131");
        assert_eq!(on_disk.source, Source::Downloaded);
        assert!(!on_disk.has_lib64, "windows tree has no lib64");
    }

    #[test]
    fn place_replaces_existing_destination() {
        let base = tempfile::tempdir().unwrap();
        let dst = base.path().join("versions").join("12.4");
        std::fs::create_dir_all(dst.join("bin")).unwrap();
        std::fs::write(dst.join("STALE"), b"old").unwrap();

        let tmp = base.path().join(".tmp-12.4");
        seed_prefix(&tmp);
        let inst = WindowsInstaller::with_paths(
            base.path().join("cache"),
            base.path().join("versions"),
            vec![],
        );
        inst.place(&tmp, &dst, &meta("12.4.131")).unwrap();

        assert!(
            !dst.join("STALE").exists(),
            "stale dst removed before rename"
        );
        assert!(dst.join("bin").join("nvcc.exe").exists());
    }

    #[test]
    fn smoke_test_is_ok_offline_on_non_windows() {
        // On the Linux lane the smoke test is a no-op stub (returns Ok); the real
        // nvcc --version probe runs only under cfg(windows).
        let base = tempfile::tempdir().unwrap();
        let inst = WindowsInstaller::with_paths(
            base.path().join("cache"),
            base.path().join("versions"),
            vec![],
        );
        assert!(inst.smoke_test(base.path()).is_ok());
    }
}

#[cfg(test)]
mod skeleton_tests {
    use super::{WindowsAcquireOutcome, WindowsInstaller};

    #[test]
    fn outcome_enum_and_helper_exist() {
        // Compile-witness: the degrade decision type is reachable, and the
        // installer can be built over an injected cache dir + dest base.
        let _i = WindowsInstaller::with_paths(
            std::path::PathBuf::from("/tmp/cache"),
            std::path::PathBuf::from("/tmp/versions"),
            vec![],
        );
        let make: fn() -> WindowsAcquireOutcome =
            || WindowsAcquireOutcome::DegradeToAdopt { reason: "x".into() };
        assert!(matches!(
            make(),
            WindowsAcquireOutcome::DegradeToAdopt { .. }
        ));
    }
}
