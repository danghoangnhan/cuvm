//! `cuvm env <spec> --shell <s>` — resolve then emit the env script to stdout.

use anyhow::Result;
use cuvm_app::Resolver;
use cuvm_core::{Os, Shell};

/// Resolve `spec` against the installed inventory and emit the activation
/// (or deactivation) script to **stdout**.  Diagnostics go to **stderr**.
///
/// - `spec == None | ""  | "."` → resolve from cwd (`.cuda-version` upward walk,
///   then fall back to the persistent `default` alias so leaving a pinned dir
///   restores the default toolkit — mirrors nvm `load-nvmrc`).
/// - Any other spec → resolve that spec directly.
/// - When nothing resolves (no pin, no default) → emit a deactivate script.
///
/// # Errors
/// Propagates resolution errors for explicit specs.  The cwd-branch silently
/// falls through to deactivate on `NotInstalled` / missing default.
pub fn run(resolver: &dyn Resolver, spec: Option<&str>, shell: Shell) -> Result<()> {
    let activator = cuvm_platform::new_activator(Os::Linux);

    let outcome = match spec {
        None | Some("" | ".") => {
            let cwd = std::env::current_dir()?;
            match resolver.resolve_from_dir(&cwd)? {
                Some(r) => Some(r),
                // No .cuda-version in scope: fall back to the persistent default
                // so leaving a pinned dir reverts (nvm load-nvmrc behavior).
                None => resolver.resolve("default").ok(),
            }
        }
        // "default" with nothing resolvable → deactivate (graceful, spec §5).
        Some("default") => resolver.resolve("default").ok(),
        Some(s) => Some(resolver.resolve(s)?),
    };

    let script = match outcome {
        Some(r) => activator.emit_env(&r.bundle, shell)?,
        None => activator.emit_deactivate(shell)?,
    };
    print!("{script}");
    Ok(())
}
