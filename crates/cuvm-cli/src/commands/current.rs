//! `cuvm current` — print the currently active bundle handle.

use anyhow::Result;

use crate::composition::Deps;

/// Print the handle of the currently active bundle.
///
/// Resolution order:
/// 1. `CUVM_CURRENT` env var (breadcrumb set by `cuvm use`)
/// 2. `.cuda-version` pin file (resolved upward from cwd)
/// 3. `default` alias
/// 4. Print `none` if nothing is active.
///
/// # Errors
/// Returns an error if the resolver fails with an I/O error.
pub fn run(deps: &Deps) -> Result<()> {
    if let Ok(cur) = std::env::var("CUVM_CURRENT") {
        if !cur.is_empty() {
            println!("{cur}");
            return Ok(());
        }
    }
    let cwd = std::env::current_dir()?;
    match deps.resolver.resolve_from_dir(&cwd)? {
        Some(resolved) => println!("{}", resolved.bundle.handle()),
        None => println!("none"),
    }
    Ok(())
}
