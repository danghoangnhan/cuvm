//! `cuvm adopt [--scan]` — discover and register existing toolkits in place.

use std::path::PathBuf;

use anyhow::Result;

use cuvm_app::{Installer, Inventory};
use cuvm_core::domain::Source;
use cuvm_core::manifest::BundleRecord;

/// Run `cuvm adopt --scan`: scan, adopt each candidate in place, persist to the
/// manifest, and print the adopted handles. Idempotent: re-adopting an existing
/// handle overwrites its record rather than duplicating it.
pub fn run_scan(installer: &dyn Installer, inventory: &dyn Inventory) -> Result<()> {
    let mut manifest = inventory.load()?;
    let candidates = installer.scan()?;

    for c in &candidates {
        let bundle = installer.adopt(c)?;
        let record = BundleRecord {
            version: bundle.toolkit.version.raw.clone(),
            source: Source::Adopted,
            path: bundle.toolkit.root.display().to_string(), // verbatim external path
            cudnn: None,
            components: bundle.toolkit.components.clone(),
            sha256: bundle.toolkit.checksum.clone(),
            installed_at: bundle.toolkit.installed_at,
        };
        // De-dup by handle: replace any existing record for this version.
        manifest.bundles.retain(|b| b.version != record.version);
        manifest.bundles.push(record);
        println!(
            "adopted {} ({})",
            bundle.toolkit.version.raw,
            bundle.toolkit.root.display()
        );
    }
    if candidates.is_empty() {
        println!("no adoptable CUDA toolkits found");
    } else {
        inventory.save(&manifest)?;
    }
    Ok(())
}

/// Resolve the scan root: `CUVM_SCAN_ROOT` override (tests) else `/usr/local`.
pub fn scan_root_override() -> Option<PathBuf> {
    std::env::var_os("CUVM_SCAN_ROOT").map(PathBuf::from)
}
