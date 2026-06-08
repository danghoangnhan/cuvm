//! cuvm subcommand implementations.

pub mod adopt;
pub mod env;
pub mod hook;

use anyhow::Result;
use clap::{Subcommand, ValueEnum};

use cuvm_core::domain::Os;
use cuvm_core::Shell;

/// clap-facing mirror of `cuvm_core::Shell` (keeps the `ValueEnum` derive out of core).
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ShellArg {
    Bash,
    Zsh,
    #[value(name = "powershell")]
    PowerShell,
    Cmd,
}

impl From<ShellArg> for Shell {
    fn from(s: ShellArg) -> Self {
        match s {
            ShellArg::Bash => Shell::Bash,
            ShellArg::Zsh => Shell::Zsh,
            ShellArg::PowerShell => Shell::PowerShell,
            ShellArg::Cmd => Shell::Cmd,
        }
    }
}

/// Available cuvm subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Discover and register existing CUDA toolkits in place.
    Adopt {
        /// Scan well-known locations (/usr/local/cuda-*) for installs to adopt.
        #[arg(long)]
        scan: bool,
    },
    /// List installed/adopted bundles.
    Ls,
    /// De-register a bundle (adopted installs are not deleted — ADR-005).
    Uninstall {
        /// Version handle to deregister (e.g. `12.4`).
        spec: String,
    },
    /// Print cd-autoload hook glue for the given shell (shim-only).
    #[command(hide = true)]
    Hook {
        #[arg(long, value_enum)]
        shell: ShellArg,
    },
    /// Print the env-mutation script for `<spec>` (shim-only).
    #[command(hide = true)]
    Env {
        /// Version spec: exact/minor/major/latest/alias/default, or empty for cwd.
        spec: Option<String>,
        #[arg(long, value_enum)]
        shell: ShellArg,
    },
}

impl Command {
    /// Dispatch the subcommand with the given inventory.
    ///
    /// # Errors
    /// Propagates any I/O or logic error from the subcommand handler.
    pub fn run(self, inventory: &dyn cuvm_app::Inventory) -> Result<()> {
        match self {
            Command::Adopt { scan } => {
                if scan {
                    let installer = build_installer();
                    adopt::run_scan(installer.as_ref(), inventory)
                } else {
                    eprintln!(
                        "cuvm adopt: pass --scan to discover and register existing system CUDA installs"
                    );
                    Ok(())
                }
            }
            Command::Ls => list(inventory),
            Command::Uninstall { spec } => {
                inventory.deregister(&spec)?;
                println!("deregistered {spec}");
                Ok(())
            }
            Command::Hook { shell } => hook::run(shell.into()),
            Command::Env { spec, shell } => {
                let resolver = crate::wiring::resolver()?;
                env::run(resolver.as_ref(), spec.as_deref(), shell.into())
            }
        }
    }
}

/// Build the unix installer, honouring `CUVM_SCAN_ROOT` (tests) over `/usr/local`.
fn build_installer() -> Box<dyn cuvm_app::Installer> {
    let platform = cuvm_core::Platform {
        os: Os::Linux,
        arch: cuvm_core::Arch::X86_64,
    };
    match adopt::scan_root_override() {
        #[cfg(unix)]
        Some(root) => Box::new(cuvm_platform::unix::UnixInstaller::with_scan_root(
            root, platform,
        )),
        _ => cuvm_platform::new_installer(Os::Linux),
    }
}

fn list(inventory: &dyn cuvm_app::Inventory) -> Result<()> {
    let bundles = inventory.list()?;
    for b in &bundles {
        println!(
            "{}\t{:?}\t{}",
            b.toolkit.version.raw,
            b.toolkit.source,
            b.toolkit.root.display()
        );
    }
    if bundles.is_empty() {
        println!("no toolkits installed");
    }
    Ok(())
}
