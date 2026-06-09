//! `cuvm doctor` — diagnose driver/toolkit/PATH health; machine-readable exit code.

use anyhow::Result;
use cuvm_app::doctor::{run_doctor, EnvSnapshot};
use cuvm_core::Version;

use crate::composition::Deps;

/// Run the `doctor` use-case: probe the driver, read the env, run diagnostics,
/// print the report, and return the machine-readable exit code.
///
/// Exit codes: 0 = all OK, 1 = warnings only, 2 = at least one block.
///
/// # Errors
/// Returns an error only if driver probing fails unexpectedly or manifest I/O
/// errors. (These are infrastructure failures, not diagnostic findings.)
pub fn run(deps: &Deps) -> Result<i32> {
    // Probe the driver (graceful-absent: probe returns present=false, never errors hard).
    let driver = deps.driver.probe()?;

    // Determine the active toolkit version: CUVM_CURRENT breadcrumb -> resolve_from_dir.
    let active: Option<Version> = active_version(deps)?;

    let env = EnvSnapshot {
        path: std::env::var("PATH").unwrap_or_default(),
        ld_library_path: std::env::var("LD_LIBRARY_PATH").unwrap_or_default(),
        cuda_home: std::env::var("CUDA_HOME").ok().filter(|s| !s.is_empty()),
        active_root: active_root(deps, active.as_ref())?,
        path_sep: path_sep(),
    };

    let report = run_doctor(deps.compat.as_ref(), &driver, active.as_ref(), &env);
    print!("{report}");
    println!();
    Ok(report.exit_code())
}

fn active_version(deps: &Deps) -> Result<Option<Version>> {
    if let Ok(cur) = std::env::var("CUVM_CURRENT") {
        if !cur.is_empty() {
            return Ok(Some(Version::parse(&cur)?));
        }
    }
    let cwd = std::env::current_dir()?;
    match deps.resolver.resolve_from_dir(&cwd)? {
        Some(r) => Ok(Some(r.bundle.toolkit.version.clone())),
        None => Ok(None),
    }
}

fn active_root(deps: &Deps, active: Option<&Version>) -> Result<Option<String>> {
    let Some(active) = active else {
        return Ok(None);
    };
    let manifest = deps.inventory.load()?;
    for b in &manifest.bundles {
        if b.version == active.raw {
            return Ok(Some(b.path.clone()));
        }
    }
    // Fall back to the conventional downloaded path.
    Ok(Some(
        deps.home
            .join("versions")
            .join(&active.raw)
            .to_string_lossy()
            .into_owned(),
    ))
}

fn path_sep() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}
