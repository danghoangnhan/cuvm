use std::path::{Path, PathBuf};

use anyhow::Result;
use cuvm_app::{AcquirePlan, Activator, ArtifactKind, Cached, Candidate, Installer};
use cuvm_core::{Bundle, Shell, VersionMeta};

use crate::not_impl;

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

/// Unix Installer. WU-1 = stub.
#[derive(Debug, Default)]
pub struct UnixInstaller;

impl UnixInstaller {
    #[must_use]
    pub fn new() -> Self {
        UnixInstaller
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
        Err(not_impl("UnixInstaller::scan"))
    }
    fn adopt(&self, _c: &Candidate) -> Result<Bundle> {
        Err(not_impl("UnixInstaller::adopt"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_app::{Activator, Installer};
    use cuvm_core::Shell;

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
        let i = UnixInstaller::new();
        let err = i.scan().unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
        let err = i.smoke_test(std::path::Path::new("/nope")).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
    }
}
