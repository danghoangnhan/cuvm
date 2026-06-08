//! Embedded Unix shell shims. Paths are relative to this source file.
//! (Windows shims ps1/cmd are added in WU-9.)

/// `cuvm.sh` — bash shim: `cuvm()` wrapper + `__cuvm_autoload` hook function.
pub const BASH_SHIM: &str = include_str!("../../../shims/cuvm.sh");

/// `cuvm.zsh` — zsh shim: `cuvm()` wrapper + `__cuvm_autoload` hook function.
pub const ZSH_SHIM: &str = include_str!("../../../shims/cuvm.zsh");
