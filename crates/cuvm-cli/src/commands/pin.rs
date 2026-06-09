//! `cuvm pin <spec>` — write `.cuda-version` in the current directory.

use anyhow::Result;

use crate::composition::Deps;

/// Validate the spec resolves, then write it to `.cuda-version` in cwd.
///
/// # Errors
/// Returns an error if the spec is unresolvable or the file cannot be written.
pub fn run(deps: &Deps, spec: &str) -> Result<()> {
    // Validate before writing so we never pin an unresolvable spec.
    deps.resolver.resolve(spec)?;
    let cwd = std::env::current_dir()?;
    let file = cwd.join(".cuda-version");
    std::fs::write(&file, format!("{spec}\n"))?;
    eprintln!("cuvm: pinned {spec} in {}", file.display());
    Ok(())
}
