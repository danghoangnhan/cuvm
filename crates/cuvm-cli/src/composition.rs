//! Composition root: the only place that knows concrete types.
//! Builds fully-wired dependencies from env/fs.

use std::collections::BTreeMap;
use std::path::PathBuf;

use cuvm_app::{Activator, CompatEngine, DriverProbe, Inventory, MemResolver, Resolver};
use cuvm_core::Os;
use cuvm_store::{FsInventory, Layout};

/// Concrete, fully-wired dependencies.
pub struct Deps {
    pub home: PathBuf,
    pub os: Os,
    pub inventory: Box<dyn Inventory>,
    pub resolver: Box<dyn Resolver>,
    pub activator: Box<dyn Activator>,
    pub compat: Box<dyn CompatEngine>,
    pub driver: Box<dyn DriverProbe>,
}

/// Build a fully-wired `Deps` from the process environment.
///
/// # Errors
/// Returns an error if the home directory cannot be resolved or the manifest
/// cannot be read.
pub fn build() -> anyhow::Result<Deps> {
    let home = cuvm_home();
    let os = host_os();
    let layout = Layout::new(&home);
    let inventory: Box<dyn Inventory> = Box::new(FsInventory::new(layout.clone()));
    let resolver = build_resolver(&layout)?;
    let activator = cuvm_platform::new_activator(os);
    let compat = cuvm_app::new_compat_engine();
    let driver = cuvm_nvidia::new_driver_probe();
    Ok(Deps {
        home,
        os,
        inventory,
        resolver,
        activator,
        compat,
        driver,
    })
}

fn build_resolver(layout: &Layout) -> anyhow::Result<Box<dyn Resolver>> {
    let inv = FsInventory::new(layout.clone());
    let manifest = cuvm_app::Inventory::load(&inv)?;
    let bundles = cuvm_app::Inventory::list(&inv)?;
    let aliases: BTreeMap<String, String> = manifest.aliases;
    Ok(Box::new(MemResolver::new(bundles, aliases)))
}

/// Base URL for the NVIDIA redist registry, overridable via `CUVM_REGISTRY_URL`
/// (tests point this at an `httpmock` server). Trailing slash is required because
/// artifact URLs are formed as `base_url + relative_path`.
#[must_use]
pub fn registry_base_url() -> String {
    std::env::var("CUVM_REGISTRY_URL").unwrap_or_else(|_| {
        "https://developer.download.nvidia.com/compute/cuda/redist/".to_string()
    })
}

/// cuDNN redist base URL: `CUVM_CUDNN_REGISTRY_URL` env override (tests) or
/// NVIDIA's production cuDNN redist. Trailing slash required for the same
/// reason as [`registry_base_url`].
#[must_use]
pub fn cudnn_registry_base_url() -> String {
    std::env::var("CUVM_CUDNN_REGISTRY_URL").unwrap_or_else(|_| {
        "https://developer.download.nvidia.com/compute/cudnn/redist/".to_string()
    })
}

/// NCCL redist base URL: `CUVM_NCCL_REGISTRY_URL` env override (tests) or
/// NVIDIA's production NCCL redist (a directory index, no manifest). Trailing
/// slash required for the same reason as [`registry_base_url`].
#[must_use]
pub fn nccl_registry_base_url() -> String {
    std::env::var("CUVM_NCCL_REGISTRY_URL").unwrap_or_else(|_| {
        "https://developer.download.nvidia.com/compute/redist/nccl/".to_string()
    })
}

/// The download cache directory: `$CUVM_HOME/cache`.
#[must_use]
pub fn cache_dir(home: &std::path::Path) -> PathBuf {
    home.join("cache")
}

/// GitHub API base for `cuvm self update`'s latest-release lookup, overridable
/// via `CUVM_SELF_UPDATE_API` (tests point this at an `httpmock` server). The
/// URL fetched is `<base>/releases/latest`, so no trailing slash.
#[must_use]
pub fn self_update_api_base() -> String {
    std::env::var("CUVM_SELF_UPDATE_API")
        .unwrap_or_else(|_| "https://api.github.com/repos/danghoangnhan/cuvm".to_string())
}

/// Release-asset base for `cuvm self update`, sharing the `CUVM_DOWNLOAD_BASE`
/// knob documented by `install.sh`/`install.ps1` (mirror/air-gapped hosts, and
/// tests). Assets are fetched as `<base>/v<ver>/<asset>`, so no trailing slash.
#[must_use]
pub fn release_download_base() -> String {
    std::env::var("CUVM_DOWNLOAD_BASE")
        .unwrap_or_else(|_| "https://github.com/danghoangnhan/cuvm/releases/download".to_string())
}

/// Resolve `CUVM_HOME` from the environment, with `~/.cuvm` as fallback.
#[must_use]
pub fn cuvm_home() -> PathBuf {
    if let Ok(h) = std::env::var("CUVM_HOME") {
        return PathBuf::from(h);
    }
    #[cfg(unix)]
    {
        let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join(".cuvm")
    }
    #[cfg(not(unix))]
    {
        let base = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join(".cuvm")
    }
}

fn host_os() -> Os {
    #[cfg(windows)]
    {
        Os::Windows
    }
    #[cfg(not(windows))]
    {
        Os::Linux
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes the tests that mutate the process-global registry env vars
    /// (`CUVM_REGISTRY_URL`, `CUVM_CUDNN_REGISTRY_URL`), so they cannot race
    /// each other under cargo's parallel test threads.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn registry_base_url_defaults_to_nvidia_redist() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::remove_var("CUVM_REGISTRY_URL");
        assert_eq!(
            registry_base_url(),
            "https://developer.download.nvidia.com/compute/cuda/redist/"
        );
    }

    #[test]
    fn registry_base_url_env_override_wins() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var("CUVM_REGISTRY_URL", "http://127.0.0.1:9/redist/");
        assert_eq!(registry_base_url(), "http://127.0.0.1:9/redist/");
        std::env::remove_var("CUVM_REGISTRY_URL");
    }

    #[test]
    fn cudnn_registry_base_url_honours_the_env_override() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var("CUVM_CUDNN_REGISTRY_URL", "http://127.0.0.1:9/cudnn/");
        let got = cudnn_registry_base_url();
        std::env::remove_var("CUVM_CUDNN_REGISTRY_URL");
        assert_eq!(got, "http://127.0.0.1:9/cudnn/");
        assert_eq!(
            cudnn_registry_base_url(),
            "https://developer.download.nvidia.com/compute/cudnn/redist/"
        );
    }

    #[test]
    fn cache_dir_is_under_home() {
        let dir = cache_dir(std::path::Path::new("/tmp/cuvmhome"));
        assert_eq!(dir, std::path::Path::new("/tmp/cuvmhome/cache"));
    }
}
