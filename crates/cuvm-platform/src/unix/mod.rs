//! Unix (Linux/WSL) backend: runtime Activator + the download/install Installer.

pub mod activator;
pub mod adopt;
pub mod installer;

pub use activator::UnixActivator;
pub use installer::UnixInstaller;

#[cfg(test)]
mod tests {
    use super::installer::UnixInstaller;
    use cuvm_app::Installer;
    use cuvm_core::{Arch, Os, Platform};

    #[test]
    fn unix_installer_acquire_is_not_implemented_until_13_2() {
        let platform = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        let i = UnixInstaller::new(platform);
        let err = i
            .acquire(&cuvm_app::AcquirePlan {
                artifacts: Vec::new(),
                dest_handle: "12.4.1".into(),
            })
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not implemented"));
    }
}
