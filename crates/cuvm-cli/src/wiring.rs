//! Composition-root helpers: build concrete implementations from env/fs.

use std::collections::BTreeMap;

use anyhow::Result;
use cuvm_app::{MemResolver, Resolver};
use cuvm_store::{FsInventory, Layout};

/// Build an inventory-backed [`Resolver`] from `CUVM_HOME` (or `~/.cuvm`).
///
/// Loads the manifest, extracts the bundle list and alias map, and wraps
/// them in a [`MemResolver`].
///
/// # Errors
/// Returns an error if the layout cannot be resolved or the manifest cannot
/// be read.
pub fn resolver() -> Result<Box<dyn Resolver>> {
    let layout = Layout::resolve_with(|k| std::env::var(k).ok(), dirs::home_dir())?;
    let inventory = FsInventory::new(layout);
    let manifest = cuvm_app::Inventory::load(&inventory)?;
    let bundles: Vec<cuvm_core::Bundle> = cuvm_app::Inventory::list(&inventory)?;
    let aliases: BTreeMap<String, String> = manifest.aliases;
    Ok(Box::new(MemResolver::new(bundles, aliases)))
}
