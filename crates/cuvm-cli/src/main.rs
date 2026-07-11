//! cuvm binary — composition root.

use cuvm_cli::cli::Cli;
use cuvm_cli::commands::{self_uninstall, Command, SelfCommand};
use cuvm_cli::composition;

fn main() {
    let exit = real_main();
    std::process::exit(exit);
}

fn real_main() -> i32 {
    let args = Cli::parse_args();

    let Some(cmd) = args.command else {
        // No subcommand; clap prints help via --help already.
        // If launched with no args, print a hint.
        eprintln!("cuvm: run `cuvm --help` for usage");
        return 1;
    };

    // `self uninstall` runs before the full wiring: `build()` parses the manifest
    // and would fail on a corrupt/half-written install — exactly the state you
    // want to be able to wipe. It needs only the `$CUVM_HOME` path.
    if let Command::SelfManage {
        command: SelfCommand::Uninstall { yes },
    } = &cmd
    {
        return to_code(self_uninstall::run(&composition::cuvm_home(), *yes));
    }

    let deps = match composition::build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cuvm: failed to initialize: {e:#}");
            return 1;
        }
    };
    to_code(cmd.run(&deps))
}

/// Map a handler's `Result<exit code>` onto a process exit code, printing the
/// error to stderr on failure.
fn to_code(result: anyhow::Result<i32>) -> i32 {
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cuvm: {e:#}");
            1
        }
    }
}
