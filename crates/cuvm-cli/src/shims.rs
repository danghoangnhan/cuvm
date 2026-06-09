//! Embedded shell shims. Paths are relative to this source file.

/// `cuvm.sh` — bash shim: `cuvm()` wrapper + `__cuvm_autoload` hook function.
pub const BASH_SHIM: &str = include_str!("../../../shims/cuvm.sh");

/// `cuvm.zsh` — zsh shim: `cuvm()` wrapper + `__cuvm_autoload` hook function.
pub const ZSH_SHIM: &str = include_str!("../../../shims/cuvm.zsh");

/// PowerShell module function (dot-sourced into `$PROFILE`).
#[must_use]
pub fn windows_powershell() -> &'static str {
    include_str!("../../../shims/cuvm.ps1")
}

/// cmd.exe shim (degraded shell: manual `cuvm use` only, no cd-hook).
#[must_use]
pub fn windows_cmd() -> &'static str {
    include_str!("../../../shims/cuvm.cmd")
}
