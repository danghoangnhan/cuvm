//! cuvm-platform — per-OS Activator/Installer backends behind a runtime factory.
//!
//! WU-1: stub backends returning `NotImplemented`. Real syscalls (registry,
//! junction, broadcast, symlink) arrive behind `#[cfg]` in WU-5/WU-9/WU-13/WU-14.

pub mod unix;
pub mod windows;

use cuvm_app::{Activator, Installer};
use cuvm_core::{Arch, Os, Platform};

use crate::unix::{UnixActivator, UnixInstaller};
use crate::windows::{WindowsActivator, WindowsInstaller};

/// Stable "not implemented yet" error for WU-1 stubs.
pub(crate) fn not_impl(what: &str) -> anyhow::Error {
    anyhow::anyhow!("{what}: not implemented (WU-1 stub)")
}

/// Runtime factory: select the Activator backend by `Os` value (not `#[cfg]`),
/// so every backend compiles on every host and Windows golden tests run on Linux CI.
#[must_use]
pub fn new_activator(os: Os) -> Box<dyn Activator> {
    match os {
        Os::Linux => Box::new(UnixActivator::new()),
        Os::Windows => Box::new(WindowsActivator::new()),
    }
}

/// Runtime factory: select the Installer backend by `Os` value.
#[must_use]
pub fn new_installer(os: Os) -> Box<dyn Installer> {
    let platform = Platform { os, arch: Arch::X86_64 };
    match os {
        Os::Linux => Box::new(UnixInstaller::new(platform)),
        Os::Windows => Box::new(WindowsInstaller::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_app::{Activator, Installer};
    use cuvm_core::{Os, Shell};

    #[test]
    fn new_activator_returns_boxed_trait_object_for_both_os() {
        let _linux: Box<dyn Activator> = new_activator(Os::Linux);
        let _win: Box<dyn Activator> = new_activator(Os::Windows);
    }

    #[test]
    fn new_installer_returns_boxed_trait_object_for_both_os() {
        let _linux: Box<dyn Installer> = new_installer(Os::Linux);
        let _win: Box<dyn Installer> = new_installer(Os::Windows);
    }

    #[test]
    fn factory_dispatches_activator_by_os() {
        // Table: (Os, shell-only-the-matching-backend-supports)
        let cases = [
            (Os::Linux, Shell::Bash, Shell::PowerShell),
            (Os::Windows, Shell::PowerShell, Shell::Bash),
        ];
        for (os, supported, foreign) in cases {
            let a = new_activator(os);
            assert!(
                a.supports(supported),
                "{os:?} backend must support its own shell"
            );
            assert!(
                !a.supports(foreign),
                "{os:?} backend must not claim the other OS's shell"
            );
        }
    }

    #[test]
    fn factory_dispatches_installer_by_os() {
        // Linux scan now works (scan_root /usr/local may be empty, but does not error).
        // Just confirm the factory returns a usable installer for each OS.
        let _linux = new_installer(Os::Linux);
        let win_err = new_installer(Os::Windows).scan().unwrap_err().to_string();
        assert!(win_err.contains("WindowsInstaller"));
    }
}
