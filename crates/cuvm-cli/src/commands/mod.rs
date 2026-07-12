//! cuvm subcommand implementations.

pub mod adopt;
pub mod alias;
pub mod completions;
pub mod cudnn;
pub mod current;
pub mod default;
pub mod doctor;
pub mod env;
pub mod exec;
pub mod hook;
pub mod install;
pub mod list;
pub mod nccl;
pub mod pin;
pub mod self_uninstall;
pub mod shell;
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

/// Output format for `cuvm ls`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
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
    /// List installed and available CUDA toolkits (installed + `<download available>`).
    Ls {
        /// Optional version filter (exact/minor/major prefix).
        spec: Option<String>,
        /// Show only installed toolkits (offline; the M1 `ls` behaviour).
        #[arg(long)]
        only_installed: bool,
        /// Show only available downloads (live fetch + cache refresh; == `ls-remote`).
        #[arg(long, conflicts_with = "only_installed")]
        only_downloads: bool,
        /// Include old patch releases (default collapses available to newest patch/minor).
        #[arg(long)]
        all_versions: bool,
        /// Show the redist URL for available rows instead of `<download available>`.
        #[arg(long)]
        show_urls: bool,
        /// Force a live fetch + cache refresh before rendering.
        #[arg(long)]
        refresh: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output_format: OutputFormat,
    },
    /// List toolkit versions available in the remote registry (alias for `ls --only-downloads`).
    ///
    /// With `--cudnn`, lists cuDNN redist versions instead.
    LsRemote {
        /// Optional version filter (exact/minor/major prefix).
        spec: Option<String>,
        /// List cuDNN versions from the cuDNN redist instead of toolkits.
        ///
        /// The cuDNN listing is always all-versions and carries no redist-URL
        /// column, so it conflicts with `--all-versions`/`--show-urls` rather
        /// than silently ignoring them.
        #[arg(long, conflicts_with_all = ["all_versions", "show_urls", "nccl"])]
        cudnn: bool,
        /// List NCCL versions from the NCCL redist instead of toolkits.
        ///
        /// Like `--cudnn`, NCCL listing is index-only (no URL column / patch
        /// collapse), so it conflicts with `--all-versions`/`--show-urls`.
        #[arg(long, conflicts_with_all = ["all_versions", "show_urls"])]
        nccl: bool,
        /// Include old patch releases (default collapses to the newest patch per minor).
        #[arg(long)]
        all_versions: bool,
        /// Show each row's redist URL instead of `<download available>`.
        #[arg(long)]
        show_urls: bool,
    },
    /// Download and install one or more CUDA toolkit versions.
    Install {
        /// Version spec(s): exact (`12.4.1`), minor (`12.4`), major (`12`), or `latest`.
        #[arg(required = true, num_args = 1..)]
        specs: Vec<String>,
        /// Reinstall even if the version is already installed (replaces the existing install; verified cached downloads are reused).
        #[arg(long, short = 'r')]
        reinstall: bool,
        /// Pair this specific cuDNN version instead of the matrix default.
        #[arg(long)]
        cudnn: Option<String>,
        /// Skip cuDNN pairing for this install.
        #[arg(long, conflicts_with = "cudnn")]
        no_cudnn: bool,
        /// Accept the NVIDIA cuDNN EULA non-interactively (recorded once
        /// under `~/.cuvm/eula/`).
        #[arg(long)]
        accept_eula: bool,
        /// Install even if the toolkit exceeds the driver ceiling.
        #[arg(long)]
        force: bool,
        /// Also install these CUDA math libraries (comma-separated or repeated):
        /// libcublas, libcufft, libcurand, libcusolver, libcusparse, libnpp,
        /// libnvjitlink. They merge into the toolkit and surface in `cuvm ls`.
        #[arg(long = "with", value_name = "COMP", value_delimiter = ',')]
        with: Vec<String>,
    },
    /// Manage cuDNN payloads paired with installed toolkits.
    Cudnn {
        #[command(subcommand)]
        command: CudnnCommand,
    },
    /// Manage NCCL companion payloads paired with installed toolkits.
    Nccl {
        #[command(subcommand)]
        command: NcclCommand,
    },
    /// Print the currently active bundle handle.
    Current,
    /// Print the absolute toolkit root for a spec.
    Which(WhichArgs),
    /// Print env-activation code for a spec (shim evals it).
    Use(UseArgs),
    /// Run a command with a toolkit activated (`cuvm exec <spec> -- <cmd> …`).
    Exec {
        /// Version spec to activate (exact/minor/major/latest/alias).
        spec: String,
        /// The command and arguments to run, given after `--`.
        #[arg(last = true, value_name = "CMD")]
        command: Vec<String>,
    },
    /// Launch a subshell with a toolkit activated (exit the shell to return).
    Shell {
        /// Optional spec; omitted => resolve from .cuda-version / default.
        spec: Option<String>,
    },
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
    /// Manage the cuvm installation itself (uninstall).
    #[command(name = "self")]
    SelfManage {
        #[command(subcommand)]
        command: SelfCommand,
    },
    /// Generate a shell completion script (bash/zsh/fish/powershell/elvish).
    Completions {
        /// Target shell for the completion script.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
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

/// `cuvm cudnn <...>` (spec §7).
#[derive(Debug, Subcommand)]
pub enum CudnnCommand {
    /// Download a cuDNN (or ingest a local redist archive) and link it into
    /// an installed toolkit.
    Install {
        /// Version spec (`9.8`, `9.8.0`, `latest`) or a path to a local
        /// `cudnn-<platform>-<ver>_cuda<major>-archive.{tar.xz,zip}`.
        what: String,
        /// Installed toolkit to pair with (e.g. `12.4.1`, or `12.4`).
        #[arg(long = "for", value_name = "TOOLKIT")]
        for_toolkit: String,
        /// Accept the NVIDIA cuDNN EULA non-interactively (recorded once
        /// under `~/.cuvm/eula/`).
        #[arg(long)]
        accept_eula: bool,
    },
    /// List cuDNN payloads in the content store and their toolkits.
    Ls,
}

/// `cuvm nccl <...>` (spec §2.3, WU-20). BSD-licensed → no EULA gate.
#[derive(Debug, Subcommand)]
pub enum NcclCommand {
    /// Download an NCCL (or ingest a local `.txz` archive) and link it into an
    /// installed toolkit. The NCCL build is selected for the toolkit's CUDA major.
    Install {
        /// Version spec (`2.21`, `2.21.5`, `latest`) or a path to a local
        /// `nccl_<ver>-<build>+cuda<major.minor>_<arch>.txz`.
        what: String,
        /// Installed toolkit to pair with (e.g. `12.4.1`, or `12.4`).
        #[arg(long = "for", value_name = "TOOLKIT")]
        for_toolkit: String,
    },
    /// List NCCL payloads in the content store and their toolkits.
    Ls,
}

/// `cuvm self <...>` — manage the cuvm install itself.
#[derive(Debug, Subcommand)]
pub enum SelfCommand {
    /// Remove the cuvm binary and data dir (`~/.cuvm`, incl. installed toolkits).
    Uninstall {
        /// Skip the interactive confirmation prompt.
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

impl Command {
    /// Dispatch the subcommand with the full wired deps.
    ///
    /// Returns the process exit code (0 = success, non-zero = error/diagnostic).
    ///
    /// # Errors
    /// Propagates any I/O or logic error from the subcommand handler.
    #[allow(clippy::too_many_lines)] // a flat dispatch over every subcommand variant
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
            Command::Ls {
                spec,
                only_installed,
                only_downloads,
                all_versions,
                show_urls,
                refresh,
                output_format,
            } => {
                let registry = build_registry();
                list::run_list(
                    deps,
                    registry.as_ref(),
                    &list::ListOpts {
                        spec,
                        only_installed,
                        only_downloads,
                        all_versions,
                        show_urls,
                        refresh,
                        json: matches!(output_format, OutputFormat::Json),
                    },
                )?;
                Ok(0)
            }
            Command::LsRemote {
                spec,
                cudnn,
                nccl,
                all_versions,
                show_urls,
            } => {
                let registry = build_registry();
                if cudnn {
                    list::run_list_cudnn_remote(registry.as_ref(), spec.as_deref())?;
                } else if nccl {
                    list::run_list_nccl_remote(registry.as_ref(), spec.as_deref())?;
                } else {
                    list::run_list(
                        deps,
                        registry.as_ref(),
                        &list::ListOpts {
                            spec,
                            only_installed: false,
                            only_downloads: true,
                            all_versions,
                            show_urls,
                            refresh: false,
                            json: false,
                        },
                    )?;
                }
                Ok(0)
            }
            Command::Install {
                specs,
                reinstall,
                cudnn,
                no_cudnn,
                accept_eula,
                force,
                with,
            } => {
                let registry = build_registry();
                let installer = build_pipeline_installer(&deps.home);
                let code = install::run_install(
                    registry.as_ref(),
                    installer.as_ref(),
                    deps.inventory.as_ref(),
                    deps.compat.as_ref(),
                    deps.driver.as_ref(),
                    &deps.home,
                    &specs,
                    reinstall,
                    force,
                    &with,
                    &install::CudnnOpts {
                        explicit: cudnn,
                        skip: no_cudnn,
                        accept_eula,
                    },
                )?;
                Ok(code)
            }
            Command::Cudnn { command } => match command {
                CudnnCommand::Install {
                    what,
                    for_toolkit,
                    accept_eula,
                } => {
                    let registry = build_registry();
                    cudnn::run_cudnn_install(
                        registry.as_ref(),
                        deps.compat.as_ref(),
                        deps.inventory.as_ref(),
                        &deps.home,
                        &what,
                        &for_toolkit,
                        accept_eula,
                    )
                }
                CudnnCommand::Ls => {
                    cudnn::run_cudnn_ls(deps.inventory.as_ref(), &deps.home)?;
                    Ok(0)
                }
            },
            Command::Nccl { command } => match command {
                NcclCommand::Install { what, for_toolkit } => {
                    let registry = build_registry();
                    nccl::run_nccl_install(
                        registry.as_ref(),
                        deps.inventory.as_ref(),
                        &deps.home,
                        &what,
                        &for_toolkit,
                    )
                }
                NcclCommand::Ls => {
                    nccl::run_nccl_ls(deps.inventory.as_ref(), &deps.home)?;
                    Ok(0)
                }
            },
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
            Command::Exec { spec, command } => exec::run(deps, &spec, &command),
            Command::Shell { spec } => shell::run(deps, spec.as_deref()),
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
                install::run_uninstall(deps.inventory.as_ref(), &deps.home, &spec)?;
                Ok(0)
            }
            Command::SelfManage { command } => match command {
                SelfCommand::Uninstall { yes } => self_uninstall::run(&deps.home, yes),
            },
            Command::Completions { shell } => {
                completions::run(shell)?;
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

/// Build the registry client, honouring `CUVM_REGISTRY_URL` and
/// `CUVM_CUDNN_REGISTRY_URL` (tests/CI) over the NVIDIA defaults. The composition
/// root is the only place that knows the concrete `DefaultRegistryClient`.
fn build_registry() -> Box<dyn cuvm_app::RegistryClient> {
    Box::new(cuvm_registry::DefaultRegistryClient::with_base_urls(
        crate::composition::registry_base_url(),
        crate::composition::cudnn_registry_base_url(),
        crate::composition::nccl_registry_base_url(),
    ))
}

/// Build the download-backed installer for the install pipeline. The cache lives
/// under `$CUVM_HOME/cache`; the composition root is the only place that names the
/// concrete unix/windows installer. Each installer constructs its own `Downloader`
/// from the cache dir inside `acquire`.
fn build_pipeline_installer(home: &std::path::Path) -> Box<dyn cuvm_app::Installer> {
    let cache = crate::composition::cache_dir(home);
    #[cfg(unix)]
    {
        use cuvm_core::{Arch, Os, Platform};
        let platform = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        Box::new(
            cuvm_platform::unix::UnixInstaller::with_cache_dir(cache, platform)
                .with_reporter(crate::reporter::CliReporter::shared()),
        )
    }
    #[cfg(not(unix))]
    {
        // Default scan roots (Program Files + CUDA_PATH*) so the degrade-to-adopt
        // fallback (spec §2.2) can actually find in-place toolkits.
        let dest_base = home.join("versions");
        Box::new(
            cuvm_platform::windows::WindowsInstaller::with_default_roots(cache, dest_base)
                .with_reporter(crate::reporter::CliReporter::shared()),
        )
    }
}
