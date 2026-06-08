//! cuvm subcommand implementations.

pub mod adopt;

use anyhow::Result;
use clap::Subcommand;

use cuvm_core::domain::Os;

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
