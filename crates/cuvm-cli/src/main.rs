//! cuvm binary — composition root. WU-0: parse the root CLI; with no
//! subcommands yet there is nothing to dispatch, so success is a no-op.

// composition root: cuvm-app ports get wired starting in WU-1
use cuvm_app as _;

mod cli;

use anyhow::Result;
use cli::Cli;

// WU-1 dispatch will add fallible operations; keep `Result` return now.
#[allow(clippy::unnecessary_wraps)]
fn main() -> Result<()> {
    let _args = Cli::parse_args();
    // Subcommand dispatch lands in WU-1/WU-8. --version/--help are handled
    // by clap before this point, so an argless invocation is a no-op.
    Ok(())
}
