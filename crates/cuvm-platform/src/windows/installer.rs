//! Windows `Installer`: read-only scan + in-place adopt of CUDA toolkit trees.
//! Scan roots are injectable so the filesystem walk runs on any host against a
//! fixture tree; production reads `CUDA_PATH`/`CUDA_PATH_V*`/Program Files (§2.2).
//! Download/extract/place land in WU-14 (kept as `not_impl` stubs here).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use cuvm_app::{AcquirePlan, ArtifactKind, Cached, Candidate, Installer};
use cuvm_core::{Arch, Bundle, Os, Platform, Source, Toolkit, Version, VersionMeta};
use time::OffsetDateTime;

/// Windows installer over a set of scan roots (Program Files + `CUDA_PATH*`).
pub struct WindowsInstaller {
    roots: Vec<PathBuf>,
}

impl Default for WindowsInstaller {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsInstaller {
    /// Construct with the default Windows scan roots (read from the environment).
    #[must_use]
    pub fn new() -> Self {
        WindowsInstaller {
            roots: default_roots(),
        }
    }

    /// Construct over explicit scan roots (used by tests with a fixture tree).
    #[must_use]
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        WindowsInstaller { roots }
    }

    fn windows_platform() -> Platform {
        Platform {
            os: Os::Windows,
            arch: Arch::X86_64,
        }
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
            platform: c.platform, // Platform is Copy
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

    fn acquire(&self, _plan: &AcquirePlan) -> Result<Vec<Cached>> {
        Err(crate::not_impl("WindowsInstaller::acquire"))
    }
    fn verify(&self, _arts: &[Cached]) -> Result<()> {
        Err(crate::not_impl("WindowsInstaller::verify"))
    }
    fn extract_atomic(&self, _arts: &[Cached], _tmp: &Path) -> Result<PathBuf> {
        Err(crate::not_impl("WindowsInstaller::extract_atomic"))
    }
    fn place(&self, _tmp: &Path, _dst: &Path, _meta: &VersionMeta) -> Result<()> {
        Err(crate::not_impl("WindowsInstaller::place"))
    }
    fn smoke_test(&self, _root: &Path) -> Result<()> {
        Err(crate::not_impl("WindowsInstaller::smoke_test"))
    }
    fn ingest_supplied(&self, _file: &Path, _kind: ArtifactKind) -> Result<PathBuf> {
        Err(crate::not_impl("WindowsInstaller::ingest_supplied"))
    }
}
