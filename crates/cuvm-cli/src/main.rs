//! cuvm binary — composition root. WU-0: parse the root CLI; with no
//! subcommands yet there is nothing to dispatch, so success is a no-op.

mod cli;

use anyhow::Result;
use cli::Cli;

fn main() -> Result<()> {
    let _args = Cli::parse_args();
    // Subcommand dispatch lands in WU-1/WU-8. --version/--help are handled
    // by clap before this point, so an argless invocation is a no-op.
    Ok(())
}
