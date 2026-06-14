//! `cuvm exec <spec> -- <cmd> [args...]` — run a command with a toolkit active.
//!
//! Resolves `spec` to a bundle, builds the same activation an interactive `use`
//! would (via the pure `cuvm_core::process_env`), applies it to the child
//! process environment, and runs `<cmd>`. Unlike `use`, nothing is printed for
//! the shell to `eval`: the child simply inherits an activated environment.

use anyhow::{anyhow, Context, Result};

use crate::composition::Deps;

/// Resolve `spec`, activate its bundle in the child environment, and run
/// `command` (program + args). Returns the child's exit code.
///
/// # Errors
/// Returns an error if `command` is empty, `spec` is unresolvable, or the
/// child process fails to launch.
pub fn run(deps: &Deps, spec: &str, command: &[String]) -> Result<i32> {
    // The top-level handler (main.rs) already prefixes errors with `cuvm: `, so
    // the message stays prefix-free — matching `use.rs` and the house style.
    let (prog, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("no command given (usage: cuvm exec <spec> -- <cmd> [args...])"))?;

    let resolved = deps.resolver.resolve(spec)?;
    let plan = cuvm_core::plan_for(&resolved.bundle);
    let vars = cuvm_core::process_env(&plan, deps.os, |k| std::env::var(k).ok());

    eprintln!(
        "cuvm: exec `{prog}` with CUDA {} active",
        resolved.bundle.handle()
    );

    let mut cmd = std::process::Command::new(prog);
    cmd.args(args);
    run_child(cmd, &vars).with_context(|| format!("running `{prog}`"))
}

/// Apply `vars` (overriding any inherited values) to `cmd`, run it to
/// completion, and return its exit code. Shared with `cuvm shell`.
///
/// # Errors
/// Returns an error only if the child process cannot be spawned (e.g. the
/// program is not found); a non-zero child exit is reported via the return code.
pub(crate) fn run_child(mut cmd: std::process::Command, vars: &[cuvm_core::EnvVar]) -> Result<i32> {
    for v in vars {
        cmd.env(&v.key, &v.value);
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to launch {}", cmd.get_program().display()))?;
    Ok(exit_code_from(status))
}

/// Map a child `ExitStatus` to a process exit code, translating a Unix
/// signal death to the shell convention `128 + signal`.
pub(crate) fn exit_code_from(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128 + sig;
        }
    }
    1
}
