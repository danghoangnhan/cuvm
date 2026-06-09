//! `cuvm install` / `cuvm ls-remote` / `cuvm uninstall` — the M2 acquire pipeline.
//!
//! `--cudnn`/`--no-cudnn` parse here but are **no-ops in M2**: cuDNN pairing is M3.

use anyhow::Result;

use cuvm_app::RegistryClient;
use cuvm_core::current_platform;

/// `cuvm ls-remote`: print available toolkit versions, newest first.
///
/// # Errors
/// Returns an error if the registry index cannot be fetched or parsed.
pub fn run_ls_remote(registry: &dyn RegistryClient) -> Result<()> {
    let platform = current_platform();
    let mut versions = registry.list_toolkits(&platform)?;
    versions.sort();
    versions.reverse();
    if versions.is_empty() {
        println!("(no remote toolkits found)");
        return Ok(());
    }
    for v in &versions {
        println!("{v}");
    }
    Ok(())
}
