//! Filesystem-facing helpers for unix adopt (scan + validate). Kept out of
//! cuvm-core so core stays I/O-free.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use time::OffsetDateTime;

use cuvm_app::Candidate;
use cuvm_core::candidate::Candidate as CoreCandidate;
use cuvm_core::domain::{Bundle, Platform, Source, Toolkit};

/// A directory is a real, adoptable toolkit iff it is a directory containing
/// BOTH `bin/nvcc` and `bin/nvcc.profile`. (`nvcc.profile` is what makes the tree
/// self-locating via `$(_HERE_)`, so its presence is the relocatability signal.)
pub(crate) fn is_valid_toolkit(root: &Path) -> bool {
    root.is_dir() && root.join("bin/nvcc").is_file() && root.join("bin/nvcc.profile").is_file()
}

/// Enumerate `cuda-X.Y` candidates directly under `scan_root`, plus the resolved
/// target of a `cuda` symlink, keeping only those that validate. Results are
/// deduped by canonicalized root path so a `cuda -> cuda-12.4` symlink does not
/// double-count. A missing/unreadable scan root yields an empty vec (not an error).
pub(crate) fn scan_root(scan_root: &Path, platform: Platform) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    let Ok(entries) = fs::read_dir(scan_root) else {
        return out; // missing root => nothing to adopt
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();

        if name == "cuda" {
            // Resolve the default-install symlink to its target dir and offer THAT
            // under its real cuda-X.Y name (canonicalize gives the target path).
            if let Ok(target) = fs::canonicalize(&path) {
                if let Some(target_name) =
                    target.file_name().map(|n| n.to_string_lossy().into_owned())
                {
                    consider_entry(&target_name, target, platform, &mut out, &mut seen);
                }
            }
            continue;
        }
        consider_entry(&name, path, platform, &mut out, &mut seen);
    }

    out
}

fn consider_entry(
    name: &str,
    path: PathBuf,
    platform: Platform,
    out: &mut Vec<Candidate>,
    seen: &mut Vec<PathBuf>,
) {
    if !is_valid_toolkit(&path) {
        return;
    }
    let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.contains(&key) {
        return;
    }
    // Parse the version from the dir name using the core type.
    if let Some(core_c) = CoreCandidate::from_dir_name(name, path.clone(), platform) {
        seen.push(key);
        // Convert to the ports Candidate (version stored in version_hint).
        out.push(Candidate {
            root: path,
            version_hint: Some(core_c.version.raw),
        });
    }
}

/// Build an in-place [`Bundle`] for a scan [`Candidate`]. Does NOT copy,
/// move, or write anything under the candidate root (ADR-005: adopt in place).
pub(crate) fn adopt_candidate(c: &Candidate) -> Result<Bundle> {
    if !is_valid_toolkit(&c.root) {
        bail!(
            "{} is not a valid CUDA toolkit (missing bin/nvcc or bin/nvcc.profile)",
            c.root.display()
        );
    }
    let version_str = c.version_hint.as_deref().unwrap_or_default();
    let version = cuvm_core::Version::parse(version_str).map_err(|e| {
        anyhow::anyhow!(
            "cannot parse version {:?} for {}: {e}",
            version_str,
            c.root.display()
        )
    })?;
    let platform = cuvm_core::current_platform();
    let toolkit = Toolkit {
        version,
        source: Source::Adopted,
        root: c.root.clone(),
        platform,
        components: Vec::new(), // unknown for an adopted tree
        has_lib64: true,        // native /usr/local layout; no lib64->lib fix
        installed_at: OffsetDateTime::now_utc(),
        checksum: None, // adopted installs can't be checksum-guaranteed
    };
    Ok(Bundle {
        toolkit,
        cudnn: None,
        extra: Vec::new(),
    })
}
