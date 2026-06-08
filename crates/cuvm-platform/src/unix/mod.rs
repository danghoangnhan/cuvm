use std::path::{Path, PathBuf};

use anyhow::Result;
use cuvm_app::{AcquirePlan, Activator, ArtifactKind, Cached, Installer};
use cuvm_core::{Bundle, Candidate, Platform, Shell, VersionMeta};

use crate::not_impl;

pub mod adopt;

/// Unix (Linux/WSL) implementation of the `Installer` port.
pub struct UnixInstaller {
    /// Directory under which `cuda-X.Y` dirs (+ the `cuda` symlink) are sought.
    /// Production default is `/usr/local`; tests inject a fixture root.
    pub(crate) scan_root: PathBuf,
    /// Host platform recorded on adopted candidates.
    pub(crate) platform: Platform,
}

impl UnixInstaller {
    /// Production constructor: scans `/usr/local`.
    #[must_use]
    pub fn new(platform: Platform) -> Self {
        Self {
            scan_root: PathBuf::from("/usr/local"),
            platform,
        }
    }

    /// Test/override constructor: scans an arbitrary root (e.g. an `assert_fs` tree).
    #[must_use]
    pub fn with_scan_root(scan_root: PathBuf, platform: Platform) -> Self {
        Self {
            scan_root,
            platform,
        }
    }
}

/// Unix (`#[cfg(unix)]` syscalls land in WU-5/WU-13) Activator. WU-1 = stub.
#[derive(Debug, Default)]
pub struct UnixActivator;

impl UnixActivator {
    #[must_use]
    pub fn new() -> Self {
        UnixActivator
    }
}

impl Activator for UnixActivator {
    fn emit_env(&self, _b: &Bundle, _sh: Shell) -> Result<String> {
        Err(not_impl("UnixActivator::emit_env"))
    }
    fn emit_deactivate(&self, _sh: Shell) -> Result<String> {
        Err(not_impl("UnixActivator::emit_deactivate"))
    }
    fn hook(&self, _sh: Shell) -> Result<String> {
        Err(not_impl("UnixActivator::hook"))
    }
    fn supports(&self, sh: Shell) -> bool {
        // Stub answer (no I/O): the unix backend will support bash/zsh in WU-5.
        matches!(sh, Shell::Bash | Shell::Zsh)
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
mod tests {
    use super::*;
    use cuvm_app::{Activator, Installer};
    use cuvm_core::{Arch, Os, Shell};

    #[test]
    fn unix_activator_methods_are_not_implemented() {
        let a = UnixActivator::new();
        // supports() answers without I/O even in the stub (no panic, returns a bool).
        let _ = a.supports(Shell::Bash);
        let err = a.emit_deactivate(Shell::Bash).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
        let err = a.hook(Shell::Zsh).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
    }

    #[test]
    fn unix_installer_methods_are_not_implemented() {
        let platform = cuvm_core::Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        let i = UnixInstaller::new(platform);
        let err = i.smoke_test(std::path::Path::new("/nope")).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
    }
}
