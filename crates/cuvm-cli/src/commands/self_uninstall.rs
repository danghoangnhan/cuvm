//! `cuvm self uninstall` — remove cuvm itself.
//!
//! The installer (`install.sh`) drops exactly two things: the `cuvm` binary and
//! the data dir `$CUVM_HOME` (`~/.cuvm`, holding shims, the cache and every
//! installed toolkit). It never edits a shell rc file — it only *prints* the
//! `source …/shims/cuvm.sh` line for the user to add — so this command deletes
//! the two things it owns and reminds the user to delete that rc line by hand.
//!
//! Adopted toolkits live outside `$CUVM_HOME` and are referenced in place
//! (ADR-005); wiping the data dir forgets the reference but never touches them.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Context, Result};

/// Confirm (unless `yes`), then delete `$CUVM_HOME` and the running binary.
///
/// A non-interactive run without `--yes` refuses rather than deleting toolkits
/// unattended; an interactive "no" aborts cleanly (exit 0).
///
/// # Errors
/// Propagates a failure to remove `$CUVM_HOME`. Failing to remove the binary is
/// reported but not fatal (the data dir — the big thing — is already gone).
pub fn run(home: &Path, yes: bool) -> Result<i32> {
    if !yes {
        // stderr is unbuffered, so the prompt shows without an explicit flush
        // (mirrors the cuDNN EULA gate in `cudnn.rs`).
        let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        if !interactive {
            eprintln!(
                "cuvm: `self uninstall` deletes {} and every installed toolkit.\n\
                 Re-run with --yes to confirm (input is not a terminal).",
                home.display()
            );
            return Ok(1);
        }
        eprintln!(
            "This deletes {} and every toolkit cuvm installed.",
            home.display()
        );
        eprint!("Continue? [y/N] ");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap_or(0);
        if !matches!(line.trim(), "y" | "Y" | "yes") {
            eprintln!("cuvm: aborted.");
            return Ok(0);
        }
    }

    do_uninstall(home, std::env::current_exe().ok().as_deref())
}

/// The removal itself — no prompt, no env — so a unit test drives it with temp
/// paths instead of deleting the test binary.
fn do_uninstall(home: &Path, exe: Option<&Path>) -> Result<i32> {
    if home.exists() {
        std::fs::remove_dir_all(home).with_context(|| format!("removing {}", home.display()))?;
        println!("removed {}", home.display());
    }

    // ponytail: unix unlinks a running binary fine (the inode outlives the
    // process). Windows can't, so we just report the path — add the
    // schedule-on-reboot dance only if Windows self-uninstall is ever asked for.
    match exe {
        Some(p) => match std::fs::remove_file(p) {
            Ok(()) => println!("removed {}", p.display()),
            Err(e) => eprintln!(
                "cuvm: could not remove the binary at {} — {e}; delete it by hand.",
                p.display()
            ),
        },
        None => eprintln!("cuvm: could not locate the running binary; delete it by hand."),
    }

    println!(
        "\ncuvm is gone. If you added the shim line to your shell rc file, delete it:\n    \
         source {}/shims/cuvm.sh",
        home.display()
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    #[test]
    fn deletes_home_and_binary() {
        let tmp = assert_fs::TempDir::new().unwrap();
        let home = tmp.child(".cuvm");
        home.child("shims/cuvm.sh").touch().unwrap();
        home.child("manifest.json").write_str("{}").unwrap();
        let exe = tmp.child("bin/cuvm");
        exe.touch().unwrap();

        let code = do_uninstall(home.path(), Some(exe.path())).unwrap();

        assert_eq!(code, 0);
        assert!(!home.path().exists(), "data dir should be deleted");
        assert!(!exe.path().exists(), "binary should be deleted");
    }

    #[test]
    fn tolerates_missing_home_and_binary() {
        let tmp = assert_fs::TempDir::new().unwrap();
        // Neither path exists — a re-run / partial state must not error.
        let code = do_uninstall(&tmp.path().join("gone"), Some(&tmp.path().join("ghost"))).unwrap();
        assert_eq!(code, 0);
    }
}
