use std::path::{Path, PathBuf};

use anyhow::Result;
use cuvm_app::{AcquirePlan, Activator, ArtifactKind, Cached, Candidate, Installer};
use cuvm_core::{Bundle, Shell, VersionMeta};

use crate::not_impl;

/// Windows Activator (HKCU R-M-W + junction land in WU-9). WU-1 = stub.
#[derive(Debug, Default)]
pub struct WindowsActivator;

impl WindowsActivator {
    #[must_use]
    pub fn new() -> Self {
        WindowsActivator
    }
}

impl Activator for WindowsActivator {
    fn emit_env(&self, _b: &Bundle, _sh: Shell) -> Result<String> {
        Err(not_impl("WindowsActivator::emit_env"))
    }
    fn emit_deactivate(&self, _sh: Shell) -> Result<String> {
        Err(not_impl("WindowsActivator::emit_deactivate"))
    }
    fn hook(&self, _sh: Shell) -> Result<String> {
        Err(not_impl("WindowsActivator::hook"))
    }
    fn supports(&self, sh: Shell) -> bool {
        // cmd is a degraded shell (no reliable cd-hook); powershell is primary.
        matches!(sh, Shell::PowerShell | Shell::Cmd)
    }
}

/// Windows Installer (redist `.zip` merge lands in WU-14). WU-1 = stub.
#[derive(Debug, Default)]
pub struct WindowsInstaller;

impl WindowsInstaller {
    #[must_use]
    pub fn new() -> Self {
        WindowsInstaller
    }
}

impl Installer for WindowsInstaller {
    fn acquire(&self, _plan: &AcquirePlan) -> Result<Vec<Cached>> {
        Err(not_impl("WindowsInstaller::acquire"))
    }
    fn verify(&self, _arts: &[Cached]) -> Result<()> {
        Err(not_impl("WindowsInstaller::verify"))
    }
    fn extract_atomic(&self, _arts: &[Cached], _tmp: &Path) -> Result<PathBuf> {
        Err(not_impl("WindowsInstaller::extract_atomic"))
    }
    fn place(&self, _tmp: &Path, _dst: &Path, _meta: &VersionMeta) -> Result<()> {
        Err(not_impl("WindowsInstaller::place"))
    }
    fn smoke_test(&self, _root: &Path) -> Result<()> {
        Err(not_impl("WindowsInstaller::smoke_test"))
    }
    fn ingest_supplied(&self, _file: &Path, _kind: ArtifactKind) -> Result<PathBuf> {
        Err(not_impl("WindowsInstaller::ingest_supplied"))
    }
    fn scan(&self) -> Result<Vec<Candidate>> {
        Err(not_impl("WindowsInstaller::scan"))
    }
    fn adopt(&self, _c: &Candidate) -> Result<Bundle> {
        Err(not_impl("WindowsInstaller::adopt"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_app::{Activator, Installer};
    use cuvm_core::Shell;

    #[test]
    fn windows_activator_methods_are_not_implemented() {
        let a = WindowsActivator::new();
        let _ = a.supports(Shell::PowerShell);
        let err = a.emit_deactivate(Shell::PowerShell).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
    }

    #[test]
    fn windows_installer_methods_are_not_implemented() {
        let i = WindowsInstaller::new();
        let err = i.scan().unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
    }
}
