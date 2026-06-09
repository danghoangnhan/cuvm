//! `cuvm use [<spec>] [--shell <s>]` — emit activation env script to stdout.

use anyhow::Result;
use cuvm_core::Shell;

use crate::composition::Deps;

/// Resolve the spec (or resolve from cwd if omitted) and emit the activation
/// script for the given shell to stdout. The shell shim evals the output.
///
/// # Errors
/// Returns an error if the spec is unresolvable, the shell is unsupported,
/// or the activator fails to render the script.
pub fn run(deps: &Deps, spec: Option<&str>, shell: Shell) -> Result<()> {
    if !deps.activator.supports(shell) {
        anyhow::bail!("shell {shell:?} is not supported for activation");
    }
    let resolved = if let Some(s) = spec {
        deps.resolver.resolve(s)?
    } else {
        let cwd = std::env::current_dir()?;
        deps.resolver
            .resolve_from_dir(&cwd)?
            .ok_or_else(|| anyhow::anyhow!("no spec given and no .cuda-version / default found"))?
    };
    eprintln!(
        "cuvm: activating {} ({:?})",
        resolved.bundle.handle(),
        resolved.via
    );
    let script = deps.activator.emit_env(&resolved.bundle, shell)?;
    print!("{script}");
    Ok(())
}
