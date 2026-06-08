//! Linux/WSL (POSIX) Activator: renders bash/zsh env scripts from an
//! `EnvPlan`. Compiles on every host (runtime dispatch — spec §3); no
//! `#[cfg]` here, the syscall floor lives elsewhere.

use std::fmt::Write as _;

use anyhow::{bail, Result};
use cuvm_app::Activator;
use cuvm_core::{plan_for, Bundle, EnvPlan, Shell};

/// The awk program that drops every PATH/LD segment present in `$CUVM_INJECTED`.
/// `!($0 in d)&&NF` => keep segments not in the breadcrumb set and non-empty;
/// `/usr/lib/wsl/lib` is never a breadcrumb member, so WSL driver libs survive.
const STRIP_AWK: &str = r#"awk -v RS=: -v ORS=: -v inj="$CUVM_INJECTED" 'BEGIN{n=split(inj,a,":");for(i=1;i<=n;i++)d[a[i]]=1} !($0 in d)&&NF{print}'"#;

/// Render the bash/zsh strip block: remove prior `CUVM_INJECTED` segments from
/// PATH and `LD_LIBRARY_PATH` FIRST (spec §2.5/§8). Identical for bash and zsh.
fn render_strip() -> String {
    format!(
        "if [ -n \"${{CUVM_INJECTED:-}}\" ]; then\n\
         \x20\x20PATH=\"$(printf '%s' \"$PATH\" | {STRIP_AWK} | sed 's/:$//')\"\n\
         \x20\x20LD_LIBRARY_PATH=\"$(printf '%s' \"${{LD_LIBRARY_PATH:-}}\" | {STRIP_AWK} | sed 's/:$//')\"\n\
         fi\n",
    )
}

/// Render the full activation script for a POSIX shell from an `EnvPlan`.
fn render_env(plan: &EnvPlan) -> String {
    let mut out = render_strip();
    let path_prepend = plan.prepend_path.join(":");
    let lib_prepend = plan.prepend_lib.join(":");
    let injected = plan.injected.join(":");
    // writeln! into a String is infallible; `let _ =` silences the unused-result lint.
    let _ = writeln!(out, "export CUDA_HOME=\"{}\"", plan.cuda_home);
    let _ = writeln!(out, "export CUDA_PATH=\"{}\"", plan.cuda_path);
    let _ = writeln!(out, "export CUDAToolkit_ROOT=\"{}\"", plan.toolkit_root);
    // Prepend bin segments to PATH; use ${PATH:+:$PATH} so empty PATH does not
    // produce a trailing colon.
    let _ = writeln!(out, "export PATH=\"{path_prepend}${{PATH:+:$PATH}}\"");
    // Prepend lib64 to LD_LIBRARY_PATH, guarding the unset case with :-.
    let _ = writeln!(
        out,
        "export LD_LIBRARY_PATH=\"{lib_prepend}:${{LD_LIBRARY_PATH:-}}\""
    );
    let _ = writeln!(out, "export CUVM_CURRENT=\"{}\"", plan.current);
    // Breadcrumb: exactly the segments we prepended, colon-joined (spec §2.5).
    let _ = writeln!(out, "export CUVM_INJECTED=\"{injected}\"");
    out
}

/// Render a deactivation script: strip the prior `CUVM_INJECTED` segments and
/// clear all cuvm-owned vars. Does NOT prepend anything (spec §5 / §8).
fn render_deactivate() -> String {
    let mut out = render_strip();
    out.push_str("unset CUDA_HOME CUDA_PATH CUDAToolkit_ROOT\n");
    out.push_str("unset CUVM_CURRENT CUVM_INJECTED\n");
    out
}

/// POSIX-shell Activator. Stateless; cheap to construct per invocation.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnixActivator;

impl UnixActivator {
    #[must_use]
    pub fn new() -> Self {
        UnixActivator
    }
}

impl Activator for UnixActivator {
    fn supports(&self, sh: Shell) -> bool {
        matches!(sh, Shell::Bash | Shell::Zsh)
    }

    fn emit_env(&self, b: &Bundle, sh: Shell) -> Result<String> {
        if !self.supports(sh) {
            bail!("UnixActivator does not support {sh:?}");
        }
        let plan = plan_for(b);
        Ok(render_env(&plan))
    }

    fn emit_deactivate(&self, sh: Shell) -> Result<String> {
        if !self.supports(sh) {
            bail!("UnixActivator does not support {sh:?}");
        }
        Ok(render_deactivate())
    }

    fn hook(&self, sh: Shell) -> Result<String> {
        if !self.supports(sh) {
            bail!("UnixActivator does not support {sh:?}");
        }
        // WU-6 will replace this with the real cd-hook body.
        Ok("# cuvm hook: installed in WU-6\n".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_app::Activator;
    use cuvm_core::Shell;

    #[test]
    fn supports_only_posix_shells() {
        let a = UnixActivator::new();
        assert!(a.supports(Shell::Bash));
        assert!(a.supports(Shell::Zsh));
        assert!(!a.supports(Shell::PowerShell));
        assert!(!a.supports(Shell::Cmd));
    }
}
