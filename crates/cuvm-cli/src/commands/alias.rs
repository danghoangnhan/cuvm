//! `cuvm alias <name> <target>` / `cuvm unalias <name>` — manage aliases.

use anyhow::Result;

use crate::composition::Deps;

/// Create or update an alias: `name → target`.
///
/// # Errors
/// Returns an error if the alias cannot be persisted.
pub fn set(deps: &Deps, name: &str, target: &str) -> Result<()> {
    deps.inventory.set_alias(name, target)?;
    eprintln!("cuvm: alias {name} -> {target}");
    Ok(())
}

/// Remove an alias. Errors if the alias does not exist.
///
/// # Errors
/// Returns an error if the alias is not found or the manifest cannot be saved.
pub fn unset(deps: &Deps, name: &str) -> Result<()> {
    let mut manifest = deps.inventory.load()?;
    if manifest.aliases.remove(name).is_none() {
        anyhow::bail!("no such alias: {name}");
    }
    deps.inventory.save(&manifest)?;
    eprintln!("cuvm: removed alias {name}");
    Ok(())
}
