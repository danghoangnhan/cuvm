//! `cuvm adopt [--scan]` — discover and register existing toolkits in place.

use std::path::PathBuf;

use anyhow::Result;

use cuvm_app::{Installer, Inventory};
use cuvm_core::domain::Source;
use cuvm_core::manifest::BundleRecord;

/// Run `cuvm adopt --scan`: scan, adopt each candidate in place, persist to the
/// manifest, and print the adopted handles. Idempotent: re-adopting an existing
/// handle overwrites its record rather than duplicating it.
///
/// # Errors
/// Returns an error if scanning, adoption, or manifest I/O fails.
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

/// Adopt a single toolkit from a given path directly (no scan).
///
/// The path must point to a `cuda-X.Y[.Z]`-shaped directory (or any valid toolkit
/// root). The version is parsed from the directory name or inferred by the installer.
///
/// # Errors
/// Returns an error if the path cannot be adopted or the manifest cannot be saved.
pub fn run_path(installer: &dyn Installer, inventory: &dyn Inventory, path: &str) -> Result<()> {
    use cuvm_core::{current_platform, Candidate, Source as CoreSource, Version};

    let root = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));

    // Parse version from dir name (cuda-X.Y or cuda-X.Y.Z) or use a placeholder.
    // `0.0.0` is built directly (not via parse) so this stays panic-free.
    let placeholder = || Version {
        fields: vec![0, 0, 0],
        raw: "0.0.0".to_string(),
    };
    let dir_name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let version = if let Some(rest) = dir_name.strip_prefix("cuda-") {
        Version::parse(rest).unwrap_or_else(|_| placeholder())
    } else {
        // Try to parse the whole name as a version.
        Version::parse(dir_name).unwrap_or_else(|_| placeholder())
    };

    let candidate = Candidate {
        version,
        root,
        platform: current_platform(),
        source: CoreSource::Adopted,
    };
    let bundle = installer.adopt(&candidate)?;
    let record = BundleRecord {
        version: bundle.toolkit.version.raw.clone(),
        source: Source::Adopted,
        path: bundle.toolkit.root.display().to_string(),
        cudnn: None,
        components: bundle.toolkit.components.clone(),
        sha256: bundle.toolkit.checksum.clone(),
        installed_at: bundle.toolkit.installed_at,
    };
    let mut manifest = inventory.load()?;
    manifest.bundles.retain(|b| b.version != record.version);
    manifest.bundles.push(record);
    inventory.save(&manifest)?;
    println!(
        "adopted {} ({})",
        bundle.toolkit.version.raw,
        bundle.toolkit.root.display()
    );
    Ok(())
}

/// Resolve the scan root: `CUVM_SCAN_ROOT` override (tests) else `/usr/local`.
pub fn scan_root_override() -> Option<PathBuf> {
    std::env::var_os("CUVM_SCAN_ROOT").map(PathBuf::from)
}
