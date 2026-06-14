//! `cuvm completions <shell>` — print a shell completion script to stdout.
//!
//! Generated from the live clap command tree, so completions never drift from
//! the actual command surface. Install per your shell, e.g.:
//!
//! ```sh
//! cuvm completions bash > ~/.local/share/bash-completion/completions/cuvm
//! cuvm completions zsh  > "${fpath[1]}/_cuvm"
//! ```

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::Shell;

/// Generate and print the completion script for `shell`.
///
/// # Errors
/// Infallible in practice (writes to stdout); returns `Result` for a uniform
/// dispatch signature.
pub fn run(shell: Shell) -> Result<()> {
    let mut cmd = crate::cli::Cli::command();
    clap_complete::generate(shell, &mut cmd, "cuvm", &mut std::io::stdout());
    Ok(())
}
