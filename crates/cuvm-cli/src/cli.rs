//! cuvm command-line surface (clap derive). WU-0: root parser with
//! `--version` / `--help` only; subcommands (§7) land in WU-1/WU-8.

use clap::Parser;

use crate::commands::Command;

/// cuvm — a CUDA toolkit version manager.
#[derive(Debug, Parser)]
#[command(
    name = "cuvm",
    version,
    about = "cuvm — a CUDA toolkit version manager.",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Parse process args into the root CLI. Exits the process on
    /// `--help` / `--version` / parse error (clap's standard behavior).
    #[must_use]
    pub fn parse_args() -> Self {
        Cli::parse()
    }
}
