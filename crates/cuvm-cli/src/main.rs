//! cuvm binary — composition root.

mod cli;
mod commands;

use anyhow::Result;
use cli::Cli;
use cuvm_store::{FsInventory, Layout};

fn main() -> Result<()> {
    let args = Cli::parse_args();

    // Build the inventory from CUVM_HOME (or ~/.cuvm fallback).
    let layout = Layout::resolve_with(|k| std::env::var(k).ok(), dirs::home_dir())?;
    let inventory = FsInventory::new(layout);

    if let Some(cmd) = args.command {
        cmd.run(&inventory)?;
    }

    Ok(())
}
