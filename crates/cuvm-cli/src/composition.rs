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
