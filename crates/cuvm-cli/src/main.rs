//! cuvm binary — composition root.

use cuvm_cli::cli::Cli;

fn main() {
    let exit = real_main();
    std::process::exit(exit);
}

fn real_main() -> i32 {
    let args = Cli::parse_args();

    let deps = match cuvm_cli::composition::build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cuvm: failed to initialize: {e:#}");
            return 1;
        }
    };

    let Some(cmd) = args.command else {
        // No subcommand; clap prints help via --help already.
        // If launched with no args, print a hint.
        eprintln!("cuvm: run `cuvm --help` for usage");
        return 1;
    };
    match cmd.run(&deps) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cuvm: {e:#}");
            1
        }
    }
}
