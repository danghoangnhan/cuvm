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
    fn unix_installer_acquire_empty_plan_creates_cache_and_returns_empty() {
        // Post-13.2: `acquire` is implemented. An empty plan downloads nothing,
        // creates the cache dir, and returns an empty `Vec` (no network touched).
        let platform = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        let cache = tempfile::tempdir().unwrap();
        let cache_dir = cache.path().join("cache");
        let i = UnixInstaller::with_cache_dir(cache_dir.clone(), platform);
        let cached = i
            .acquire(&cuvm_app::AcquirePlan {
                artifacts: Vec::new(),
                dest_handle: "12.4.1".into(),
            })
            .expect("empty plan acquires cleanly");
        assert!(cached.is_empty(), "no artifacts => no cached files");
        assert!(cache_dir.is_dir(), "acquire creates the cache dir");
    }
}
