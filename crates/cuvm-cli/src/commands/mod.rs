//! cuvm subcommand implementations.

pub mod adopt;
pub mod alias;
pub mod current;
pub mod default;
pub mod doctor;
pub mod env;
pub mod hook;
pub mod install;
pub mod pin;
pub mod r#use;
pub mod which;

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

use cuvm_core::{Os, Shell};

use crate::composition::Deps;

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

/// clap-facing OS override for the hidden `env`/`hook` plumbing commands. Lets
/// the Linux CI lane drive the runtime-dispatched Windows activator (WU-9).
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OsArg {
    Linux,
    Windows,
}

impl From<OsArg> for Os {
    fn from(o: OsArg) -> Self {
        match o {
            OsArg::Linux => Os::Linux,
            OsArg::Windows => Os::Windows,
        }
    }
}

/// Resolve the effective OS for emission: explicit `--os` wins, else host OS.
fn resolve_os(os: Option<OsArg>) -> Os {
    match os {
        Some(o) => o.into(),
        None if cfg!(windows) => Os::Windows,
        None => Os::Linux,
    }
}

/// Which args for `cuvm which`.
#[derive(Args, Debug)]
pub struct WhichArgs {
    /// Version spec to look up.
    pub spec: String,
}

/// Use args for `cuvm use`.
#[derive(Args, Debug)]
pub struct UseArgs {
    /// Optional spec; omitted => resolve from .cuda-version / default.
    pub spec: Option<String>,
    /// Target shell for the emitted script.
    #[arg(long, value_enum, default_value_t = ShellArg::Bash)]
    pub shell: ShellArg,
}

/// Default args for `cuvm default`.
#[derive(Args, Debug)]
pub struct DefaultArgs {
    /// Version spec to set as default.
    pub spec: String,
    /// Also create the opt-in `current` symlink/junction pointer.
    #[arg(long)]
    pub link: bool,
}

/// Alias args for `cuvm alias`.
#[derive(Args, Debug)]
pub struct AliasArgs {
    /// Alias name.
    pub name: String,
    /// Target spec.
    pub target: String,
}

/// Unalias args for `cuvm unalias`.
#[derive(Args, Debug)]
pub struct UnaliasArgs {
    /// Alias name to remove.
    pub name: String,
}

/// Pin args for `cuvm pin`.
#[derive(Args, Debug)]
pub struct PinArgs {
    /// Version spec to pin in .cuda-version.
    pub spec: String,
}

/// Available cuvm subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Discover and register existing CUDA toolkits in place.
    Adopt {
        /// Path to an existing CUDA toolkit directory to adopt (e.g. /usr/local/cuda-12.4).
        path: Option<String>,
        /// Scan well-known locations (/usr/local/cuda-*) for installs to adopt.
        #[arg(long)]
        scan: bool,
    },
    /// List installed/adopted bundles.
    Ls,
    /// List toolkit versions available in the remote registry.
    LsRemote {
        /// List cuDNN versions instead (M2: parsed but a no-op; listing lands in M3).
        #[arg(long)]
        cudnn: bool,
    },
    /// Print the currently active bundle handle.
    Current,
    /// Print the absolute toolkit root for a spec.
    Which(WhichArgs),
    /// Print env-activation code for a spec (shim evals it).
    Use(UseArgs),
    /// Set the persistent default (writes the `default` alias).
    Default(DefaultArgs),
    /// Create or update an alias.
    Alias(AliasArgs),
    /// Remove an alias.
    Unalias(UnaliasArgs),
    /// Write `.cuda-version` in the current directory.
    Pin(PinArgs),
    /// Diagnose driver/toolkit/PATH health; exit code is machine-readable.
    Doctor,
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
        /// Override the emission OS (defaults to the host); used by CI/tests.
        #[arg(long, value_enum)]
        os: Option<OsArg>,
    },
    /// Print the env-mutation script for `<spec>` (shim-only).
    #[command(hide = true)]
    Env {
        /// Version spec: exact/minor/major/latest/alias/default, or empty for cwd.
        spec: Option<String>,
        #[arg(long, value_enum)]
        shell: ShellArg,
        /// Override the emission OS (defaults to the host); used by CI/tests.
        #[arg(long, value_enum)]
        os: Option<OsArg>,
    },
}

impl Command {
    /// Dispatch the subcommand with the full wired deps.
    ///
    /// Returns the process exit code (0 = success, non-zero = error/diagnostic).
    ///
    /// # Errors
    /// Propagates any I/O or logic error from the subcommand handler.
    pub fn run(self, deps: &Deps) -> Result<i32> {
        match self {
            Command::Adopt { path, scan } => {
                if let Some(p) = path {
                    let installer = build_installer();
                    adopt::run_path(installer.as_ref(), deps.inventory.as_ref(), &p)?;
                } else if scan {
                    let installer = build_installer();
                    adopt::run_scan(installer.as_ref(), deps.inventory.as_ref())?;
                } else {
                    eprintln!("cuvm adopt: pass a path or --scan to discover system CUDA installs");
                }
                Ok(0)
            }
            Command::Ls => {
                run_ls(deps)?;
                Ok(0)
            }
            Command::LsRemote { cudnn: _ } => {
                let registry = build_registry();
                install::run_ls_remote(registry.as_ref())?;
                Ok(0)
            }
            Command::Current => {
                current::run(deps)?;
                Ok(0)
            }
            Command::Which(a) => {
                which::run(deps, &a.spec)?;
                Ok(0)
            }
            Command::Use(a) => {
                r#use::run(deps, a.spec.as_deref(), a.shell.into())?;
                Ok(0)
            }
            Command::Default(a) => {
                default::run(deps, &a.spec, a.link)?;
                Ok(0)
            }
            Command::Alias(a) => {
                alias::set(deps, &a.name, &a.target)?;
                Ok(0)
            }
            Command::Unalias(a) => {
                alias::unset(deps, &a.name)?;
                Ok(0)
            }
            Command::Pin(a) => {
                pin::run(deps, &a.spec)?;
                Ok(0)
            }
            Command::Doctor => doctor::run(deps),
            Command::Uninstall { spec } => {
                deps.inventory.deregister(&spec)?;
                println!("deregistered {spec}");
                Ok(0)
            }
            Command::Hook { shell, os } => {
                hook::run(shell.into(), resolve_os(os))?;
                Ok(0)
            }
            Command::Env { spec, shell, os } => {
                let resolver = crate::wiring::resolver()?;
                env::run(
                    resolver.as_ref(),
                    spec.as_deref(),
                    shell.into(),
                    resolve_os(os),
                )?;
                Ok(0)
            }
        }
    }
}

/// Build the unix installer, honouring `CUVM_SCAN_ROOT` (tests) over `/usr/local`.
fn build_installer() -> Box<dyn cuvm_app::Installer> {
    // The CUVM_SCAN_ROOT override is unix-only; on other targets fall straight
    // through to the factory (keeps `platform` from being unused off-unix).
    #[cfg(unix)]
    if let Some(root) = adopt::scan_root_override() {
        use cuvm_core::{Arch, Os, Platform};
        let platform = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        return Box::new(cuvm_platform::unix::UnixInstaller::with_scan_root(
            root, platform,
        ));
    }
    cuvm_platform::new_installer(cuvm_core::Os::Linux)
}

/// Build the registry client, honouring `CUVM_REGISTRY_URL` (tests/CI) over the
/// NVIDIA default. The composition root is the only place that knows the concrete
/// `DefaultRegistryClient`.
fn build_registry() -> Box<dyn cuvm_app::RegistryClient> {
    Box::new(cuvm_registry::DefaultRegistryClient::with_base_url(
        crate::composition::registry_base_url(),
    ))
}

/// `ls` implementation using `Deps` (marks default alias with `*`).
fn run_ls(deps: &Deps) -> Result<()> {
    let manifest = deps.inventory.load()?;
    let default = manifest.aliases.get("default").cloned();
    let bundles = deps.inventory.list()?;
    if bundles.is_empty() {
        println!("(no toolkits installed)");
        return Ok(());
    }
    for b in &bundles {
        let handle = b.handle();
        if default.as_deref() == Some(handle.as_str()) {
            println!("{handle} *");
        } else {
            println!("{handle}");
        }
    }
    Ok(())
}
