//! `cuvm which <spec>` — print the absolute toolkit root path.

use std::path::Path;

use anyhow::Result;

use crate::composition::Deps;

/// Print the absolute path to the toolkit root for a version spec.
///
/// # Errors
/// Returns an error if the spec cannot be resolved.
pub fn run(deps: &Deps, spec: &str) -> Result<()> {
    let resolved = deps.resolver.resolve(spec)?;
    let root: &Path = &resolved.bundle.toolkit.root;
    let abs = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    println!("{}", abs.display());
    Ok(())
}
