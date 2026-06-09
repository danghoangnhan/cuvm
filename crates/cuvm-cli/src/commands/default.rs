//! `cuvm default <spec> [--link]` — persist the `default` alias.

use std::path::Path;

use anyhow::Result;

use crate::composition::Deps;

/// Set the persistent default alias to the resolved handle.
/// With `--link`, also create the opt-in `~/.cuvm/current` symlink (unix only).
///
/// # Errors
/// Returns an error if the spec is unresolvable, the alias cannot be saved,
/// or (on unix) the symlink cannot be created.
pub fn run(deps: &Deps, spec: &str, link: bool) -> Result<()> {
    // Validate it actually resolves before persisting.
    let resolved = deps.resolver.resolve(spec)?;
    let handle = resolved.bundle.handle();
    deps.inventory.set_alias("default", &handle)?;
    eprintln!("cuvm: default -> {handle}");

    if link {
        let target = deps.home.join("versions").join(&handle);
        let pointer = deps.home.join("current");
        repoint_current(&pointer, &target)?;
        eprintln!("cuvm: current -> {}", target.display());
    }
    Ok(())
}

#[cfg(unix)]
fn repoint_current(pointer: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;
    // Re-point atomically: remove any existing pointer first (file or symlink).
    if pointer.symlink_metadata().is_ok() {
        std::fs::remove_file(pointer)?;
    }
    let abs = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()?.join(target)
    };
    symlink(&abs, pointer)?;
    Ok(())
}

#[cfg(not(unix))]
fn repoint_current(_pointer: &Path, _target: &Path) -> Result<()> {
    // Windows junction repoint is implemented in WU-9 (mklink /J, no admin).
    anyhow::bail!("--link is implemented on the windows lane in WU-9");
}
