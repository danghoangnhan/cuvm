//! `cuvm shell [<spec>]` — launch a subshell with a toolkit active.
//!
//! Like `exec`, but the "command" is the user's interactive shell. The bundle's
//! activation is applied to the child shell's environment; exiting the shell
//! returns to the caller. With no `spec`, resolve from `.cuda-version` / the
//! persistent default (the same precedence as `use`).

use anyhow::{anyhow, Result};
use cuvm_core::Os;

use crate::commands::exec::run_child;
use crate::composition::Deps;

/// Resolve `spec` (or the cwd/default), then launch a subshell with the bundle
/// activated. Returns the subshell's exit code.
///
/// # Errors
/// Returns an error if nothing resolves, or the subshell fails to launch.
pub fn run(deps: &Deps, spec: Option<&str>) -> Result<i32> {
    let resolved = if let Some(s) = spec {
        deps.resolver.resolve(s)?
    } else {
        let cwd = std::env::current_dir()?;
        match deps.resolver.resolve_from_dir(&cwd)? {
            Some(r) => r,
            // main.rs adds the `cuvm: ` prefix; keep the message prefix-free.
            None => deps
                .resolver
                .resolve("default")
                .map_err(|_| anyhow!("no spec given and no .cuda-version / default found"))?,
        }
    };

    let plan = cuvm_core::plan_for(&resolved.bundle);
    let mut vars = cuvm_core::process_env(&plan, deps.os, |k| std::env::var(k).ok());
    // A breadcrumb so a nested prompt / the user can tell a cuvm subshell is active.
    vars.push(cuvm_core::EnvVar {
        key: "CUVM_SHELL".to_string(),
        value: resolved.bundle.handle(),
    });

    let shell_prog = pick_shell(deps.os);
    eprintln!(
        "cuvm: launching {shell_prog} with CUDA {} active — exit the shell to return",
        resolved.bundle.handle()
    );
    let cmd = std::process::Command::new(&shell_prog);
    run_child(cmd, &vars)
}

/// Pick the interactive shell to launch: `$SHELL` (Unix) / `%COMSPEC%` (Windows),
/// falling back to `/bin/sh` / `cmd.exe`.
fn pick_shell(os: Os) -> String {
    match os {
        Os::Windows => std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        Os::Linux => std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
    }
}
