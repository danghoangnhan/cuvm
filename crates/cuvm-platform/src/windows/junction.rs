//! `current` directory-junction pointer (the Windows analogue of the Unix
//! `current` symlink). §2.2 requires `mklink /J` (no admin) — not a `/D` symlink.
//!
//! The create/repoint state machine is host-neutral and tested on Linux via a
//! real directory symlink; the actual `mklink /J` call is `#[cfg(windows)]`.

use std::path::Path;

use anyhow::Result;

/// Create or re-point a directory junction (Windows) / dir symlink (test host)
/// at `link` pointing to `target`. Removes any existing link first so re-pointing
/// the cuvm "current" pointer is idempotent (§2.2).
///
/// # Errors
/// Returns an error if the existing link cannot be removed or the new link
/// cannot be created.
pub fn set_junction(link: &Path, target: &Path) -> Result<()> {
    remove_existing(link)?;
    create_dir_link(link, target)
}

fn remove_existing(link: &Path) -> Result<()> {
    // symlink_metadata so we inspect the link itself, never follow it.
    match std::fs::symlink_metadata(link) {
        Ok(_) => {
            #[cfg(windows)]
            {
                std::fs::remove_dir(link).or_else(|_| std::fs::remove_file(link))?;
            }
            #[cfg(not(windows))]
            {
                std::fs::remove_file(link).or_else(|_| std::fs::remove_dir_all(link))?;
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(windows)]
fn create_dir_link(link: &Path, target: &Path) -> Result<()> {
    // A directory junction needs no admin (unlike a /D symlink). Shell out to the
    // built-in `mklink /J` to stay within documented, admin-free behavior (§2.2).
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()?;
    anyhow::ensure!(status.success(), "mklink /J failed for {}", link.display());
    Ok(())
}

#[cfg(not(windows))]
fn create_dir_link(link: &Path, target: &Path) -> Result<()> {
    // Test-host stub: a real directory symlink reproduces the create/repoint
    // semantics so the state machine is covered on the linux lane.
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}
