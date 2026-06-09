//! Windows `Activator`: PowerShell + cmd env-script emission, deactivation, and
//! the chained `prompt()` cd-hook. Pure string building — compiles and golden-
//! tests on every host (script emission is runtime-dispatched, spec §3/§8).

use anyhow::{bail, Result};
use cuvm_app::Activator;
use cuvm_core::{Bundle, Shell};

/// Windows activation-script emitter (PowerShell primary, cmd degraded).
#[derive(Debug, Default)]
pub struct WindowsActivator;

impl WindowsActivator {
    /// Create a new Windows activator.
    #[must_use]
    pub fn new() -> Self {
        WindowsActivator
    }

    /// PowerShell activation: strip the prior `CUVM_INJECTED` segments out of
    /// `$env:Path` (`-split ';'`), set the three CUDA roots, prepend `…\bin`,
    /// rewrite the breadcrumbs (spec §8).
    fn ps_env(b: &Bundle) -> String {
        let root = b.toolkit.root.to_string_lossy().replace('/', "\\");
        let bin = format!("{root}\\bin");
        let current = b.toolkit.version.raw.clone();
        let injected = bin.clone();
        format!(
            r"if ($env:CUVM_INJECTED) {{
  $cuvm_inj = $env:CUVM_INJECTED -split ';'
  $env:Path = (($env:Path -split ';') | Where-Object {{ $_ -and ($cuvm_inj -notcontains $_) }}) -join ';'
}}
$env:CUDA_HOME = '{root}'
$env:CUDA_PATH = '{root}'
$env:CUDAToolkit_ROOT = '{root}'
$env:Path = '{bin};' + $env:Path
$env:CUVM_CURRENT = '{current}'
$env:CUVM_INJECTED = '{injected}'
"
        )
    }

    /// cmd activation: CRLF-terminated `set "NAME=VALUE"` lines (spec §8).
    fn cmd_env(b: &Bundle) -> String {
        let root = b.toolkit.root.to_string_lossy().replace('/', "\\");
        let bin = format!("{root}\\bin");
        let current = b.toolkit.version.raw.clone();
        format!(
            "set \"CUDA_HOME={root}\"\r\n\
             set \"CUDA_PATH={root}\"\r\n\
             set \"CUDAToolkit_ROOT={root}\"\r\n\
             set \"PATH={bin};%PATH%\"\r\n\
             set \"CUVM_CURRENT={current}\"\r\n\
             set \"CUVM_INJECTED={bin}\"\r\n"
        )
    }
}

impl Activator for WindowsActivator {
    fn emit_env(&self, b: &Bundle, sh: Shell) -> Result<String> {
        match sh {
            Shell::PowerShell => Ok(Self::ps_env(b)),
            Shell::Cmd => Ok(Self::cmd_env(b)),
            other => bail!("WindowsActivator does not support {other:?}"),
        }
    }

    fn emit_deactivate(&self, sh: Shell) -> Result<String> {
        match sh {
            Shell::PowerShell => Ok(r"if ($env:CUVM_INJECTED) {
  $cuvm_inj = $env:CUVM_INJECTED -split ';'
  $env:Path = (($env:Path -split ';') | Where-Object { $_ -and ($cuvm_inj -notcontains $_) }) -join ';'
}
Remove-Item Env:\CUDA_HOME -ErrorAction SilentlyContinue
Remove-Item Env:\CUDA_PATH -ErrorAction SilentlyContinue
Remove-Item Env:\CUDAToolkit_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:\CUVM_CURRENT -ErrorAction SilentlyContinue
Remove-Item Env:\CUVM_INJECTED -ErrorAction SilentlyContinue
"
            .to_string()),
            Shell::Cmd => Ok("set \"CUDA_HOME=\"\r\nset \"CUDA_PATH=\"\r\n\
                              set \"CUDAToolkit_ROOT=\"\r\nset \"CUVM_CURRENT=\"\r\n\
                              set \"CUVM_INJECTED=\"\r\n"
                .to_string()),
            other => bail!("WindowsActivator does not support {other:?}"),
        }
    }

    fn hook(&self, sh: Shell) -> Result<String> {
        match sh {
            // Chain any existing prompt (oh-my-posh/Starship) — capture it once
            // into $global:__cuvm_prev_prompt, then re-invoke it (spec §2.5).
            Shell::PowerShell => Ok(r#"if (-not (Test-Path Variable:\__cuvm_prev_prompt)) {
  $cmd = Get-Command prompt -ErrorAction SilentlyContinue
  if ($cmd) { $global:__cuvm_prev_prompt = $cmd.ScriptBlock }
}
function global:prompt {
  try { (& cuvm.exe use --shell powershell --quiet | Out-String) | Invoke-Expression } catch {}
  if ($global:__cuvm_prev_prompt) { & $global:__cuvm_prev_prompt } else { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }
}
"#
            .to_string()),
            // cmd has no reliable cd-hook (§2.5) — caller warns; body is empty.
            Shell::Cmd => Ok(String::new()),
            other => bail!("WindowsActivator does not support {other:?}"),
        }
    }

    fn supports(&self, sh: Shell) -> bool {
        // cmd is a degraded shell (no reliable cd-hook); powershell is primary.
        matches!(sh, Shell::PowerShell | Shell::Cmd)
    }
}
