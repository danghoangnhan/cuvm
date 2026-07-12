//! `cuvm self update` — replace the running cuvm binary with a newer release.
//!
//! Safety model (the whole point of this command): the installed binary is
//! **never touched until a byte-verified replacement is fully staged**. The
//! order is download → sha256-verify → extract → smoke-test (`--version`) →
//! atomic rename. Any failure before the rename leaves the install
//! byte-for-byte intact, so a dropped connection, a 404, a checksum mismatch,
//! or a bad build can never brick cuvm.
//!
//! Trust model mirrors `install.sh` / `install.ps1`: sha256 against `SHA256SUMS`
//! fetched over TLS from GitHub. (`SHA256SUMS.sig` / `.pem` are also published;
//! in-binary cosign verification is deferred — it would pull in a heavy sigstore
//! dependency for authenticity we do not assert at first-install time either.)

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use cuvm_core::Version;
use cuvm_download::{extract_tar_gz, extract_zip, http_get, Downloader};

use crate::composition;
use crate::reporter::CliReporter;

/// Run `cuvm self update`.
///
/// With `check`, reports whether a newer release exists and returns without
/// downloading anything. Otherwise resolves the target version (`version` if
/// given, else the latest release), and — when it is newer than the running
/// binary (or `force`) — downloads, verifies, and atomically swaps it in.
///
/// # Errors
/// Propagates network, checksum, extraction, or filesystem failures. Every such
/// failure occurs before the binary swap, so the current install is unchanged.
pub fn run(home: &Path, check: bool, force: bool, version: Option<&str>) -> Result<i32> {
    let current = env!("CARGO_PKG_VERSION");
    let asset = asset_name()?;

    let target = match version {
        Some(v) => v.trim_start_matches('v').to_string(),
        None => resolve_latest()?,
    };

    let cur_v = Version::parse(current).context("parsing the running cuvm version")?;
    let tgt_v =
        Version::parse(&target).with_context(|| format!("parsing target version `{target}`"))?;
    let newer = tgt_v > cur_v;

    if check {
        if newer {
            println!("cuvm {current} → {target} available (run `cuvm self update`)");
        } else if version.is_some() {
            // Do not call an explicitly-requested version "latest".
            println!("cuvm {current} is already at or ahead of {target}");
        } else {
            println!("cuvm {current} is up to date (latest {target})");
        }
        return Ok(0);
    }

    if !newer && !force {
        // Target is not newer than what is running: nothing to do without --force.
        if version.is_some() {
            println!(
                "cuvm {current} is already at or ahead of {target}; \
                 pass --force to reinstall or downgrade"
            );
        } else {
            println!("cuvm {current} is already up to date (latest {target})");
        }
        return Ok(0);
    }

    let dl_base = composition::release_download_base();
    let stage = format!("cuvm-{target}-{asset}");
    let ext = archive_ext();
    let archive = format!("{stage}.{ext}");
    let sums_url = format!("{dl_base}/v{target}/SHA256SUMS");
    let archive_url = format!("{dl_base}/v{target}/{archive}");

    eprintln!("cuvm: updating {current} → {target}…");

    // Require a checksum: we refuse to swap in unverified bytes (stricter than
    // install.sh, which skips verification when SHA256SUMS is absent).
    let sums = http_get(&sums_url)
        .with_context(|| format!("fetching {sums_url} (is v{target} a published release?)"))?;
    let expected = sha_for(&sums, &archive)?;

    let cache = composition::cache_dir(home);
    let downloader = Downloader::with_reporter(cache.clone(), CliReporter::shared());
    let archive_path = downloader
        .fetch_labeled(&archive_url, &expected, &archive, &format!("cuvm {target}"))
        .with_context(|| format!("downloading {archive_url}"))?;

    // Extract into a scratch dir under the cache, cleaned up either way.
    let work = cache.join(format!(".self-update.{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    let result = stage_and_swap(&archive_path, ext, &stage, &work, home);
    let _ = fs::remove_dir_all(&work);
    result?;

    println!("updated cuvm {current} → {target}");
    Ok(0)
}

/// Extract the archive, smoke-test the new binary, atomically swap it over the
/// installed one, then refresh the shims. Split out of [`run`] so the scratch
/// dir is cleaned whether this succeeds or fails.
fn stage_and_swap(archive: &Path, ext: &str, stage: &str, work: &Path, home: &Path) -> Result<()> {
    eprintln!("cuvm: verifying and extracting…");
    if ext == "zip" {
        extract_zip(archive, work)?;
    } else {
        extract_tar_gz(archive, work)?;
    }

    let exe = format!("cuvm{}", std::env::consts::EXE_SUFFIX);
    let new_bin = work.join(stage).join(&exe);
    let meta = fs::metadata(&new_bin)
        .with_context(|| format!("release archive is missing {stage}/{exe}"))?;
    if meta.len() == 0 {
        bail!("extracted {} is empty", new_bin.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new_bin, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("setting exec bit on {}", new_bin.display()))?;
    }

    // Smoke-test the freshly extracted binary BEFORE touching the installed one:
    // it must run and identify as cuvm (catches a corrupt or wrong-arch build).
    smoke_test(&new_bin)?;

    let target = replace_target()?;
    swap_binary(&target, &new_bin)
        .with_context(|| format!("installing the new binary over {}", target.display()))?;

    // Best-effort shim refresh (the archive is already unpacked). A stale shim
    // never fails the update — the binary swap has already succeeded.
    refresh_shims(&work.join(stage).join("shims"), &home.join("shims"));
    Ok(())
}

/// Resolve the latest published release tag (leading `v` stripped) via the
/// GitHub releases API.
fn resolve_latest() -> Result<String> {
    let url = format!("{}/releases/latest", composition::self_update_api_base());
    let body = http_get(&url).with_context(|| format!("fetching {url}"))?;
    let json: serde_json::Value =
        serde_json::from_slice(&body).context("parsing the GitHub release JSON")?;
    let tag = json
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .context("the GitHub release JSON has no `tag_name`")?;
    Ok(tag.trim_start_matches('v').to_string())
}

/// Run `<bin> --version`; error unless it exits 0 and identifies as cuvm.
fn smoke_test(bin: &Path) -> Result<()> {
    let out = std::process::Command::new(bin)
        .arg("--version")
        .output()
        .with_context(|| format!("running {} --version", bin.display()))?;
    if !out.status.success() {
        bail!(
            "downloaded binary failed its smoke test (exit {:?}); keeping the current version",
            out.status.code()
        );
    }
    if !out.stdout.starts_with(b"cuvm ") {
        bail!("downloaded binary did not identify as cuvm; keeping the current version");
    }
    Ok(())
}

/// The path to replace: the running binary, or the `CUVM_SELF_UPDATE_TARGET`
/// override (a test/advanced seam so an e2e can drive the full pipeline without
/// clobbering the running test binary).
fn replace_target() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CUVM_SELF_UPDATE_TARGET") {
        return Ok(PathBuf::from(p));
    }
    std::env::current_exe().context("locating the running cuvm binary")
}

/// The release-asset platform tag for this host (matches `install.sh`).
fn asset_name() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-amd64",
        ("linux", "aarch64") => "linux-arm64",
        ("windows", "x86_64") => "windows-amd64",
        (os, arch) => bail!(
            "no prebuilt cuvm release for {os}/{arch} \
             (have: linux-amd64, linux-arm64, windows-amd64)"
        ),
    })
}

/// Archive extension for this host's release asset.
fn archive_ext() -> &'static str {
    if cfg!(windows) {
        "zip"
    } else {
        "tar.gz"
    }
}

/// Find the expected sha256 for `archive` in a `SHA256SUMS` body
/// (`<hex>␠␠<name>` lines, a leading `*` on the name tolerated).
fn sha_for(sums: &[u8], archive: &str) -> Result<String> {
    let text = std::str::from_utf8(sums).context("SHA256SUMS is not valid UTF-8")?;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if let (Some(sha), Some(name)) = (it.next(), it.next()) {
            if name.trim_start_matches('*') == archive {
                return Ok(sha.to_string());
            }
        }
    }
    bail!("SHA256SUMS has no entry for {archive}")
}

/// Copy every regular file from `src` into `dst`, best-effort. Missing `src`
/// (older archive layout) or a copy failure is silently ignored — shims are
/// cosmetic glue and must never fail an update whose binary swap has landed.
fn refresh_shims(src: &Path, dst: &Path) {
    let Ok(entries) = fs::read_dir(src) else {
        return;
    };
    if fs::create_dir_all(dst).is_err() {
        return;
    }
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = fs::copy(entry.path(), dst.join(entry.file_name()));
        }
    }
}

/// A hidden temp sibling of `target`, used to stage the new binary in the same
/// directory (so the final rename is same-filesystem and cannot hit `EXDEV`).
fn sibling_temp(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map_or_else(|| "cuvm".to_string(), |n| n.to_string_lossy().into_owned());
    let tmp = format!(".{name}.new.{}", std::process::id());
    target
        .parent()
        .map_or_else(|| PathBuf::from(&tmp), |p| p.join(&tmp))
}

/// Copy `source` onto the staged temp, removing a partial temp if the copy
/// fails part-way (e.g. a full disk) so nothing is left beside `target`.
fn stage_copy(source: &Path, staged: &Path) -> Result<()> {
    fs::copy(source, staged).map_err(|e| {
        let _ = fs::remove_file(staged);
        anyhow::Error::new(e).context(format!("staging new binary at {}", staged.display()))
    })?;
    Ok(())
}

/// Atomically replace `target` with the bytes of `source` (unix).
///
/// Stages a copy beside `target`, sets the exec bit, then renames it into place.
/// Replacing a running binary's path is fine on unix: the running process keeps
/// the old inode, the path points at the new one. On any failure `target` is
/// untouched and the staged temp is removed.
#[cfg(unix)]
fn swap_binary(target: &Path, source: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let staged = sibling_temp(target);
    stage_copy(source, &staged)?;
    if let Err(e) = fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)) {
        let _ = fs::remove_file(&staged);
        return Err(e).with_context(|| format!("setting exec bit on {}", staged.display()));
    }
    fs::rename(&staged, target).map_err(|e| {
        let _ = fs::remove_file(&staged);
        anyhow::Error::new(e).context(format!("replacing {}", target.display()))
    })?;
    Ok(())
}

/// Atomically replace `target` with the bytes of `source` (windows).
///
/// Windows cannot overwrite a running `.exe`, so the swap goes through
/// [`swap_via_rename_aside`].
#[cfg(windows)]
fn swap_binary(target: &Path, source: &Path) -> Result<()> {
    swap_via_rename_aside(target, source)
}

/// Replace `target` by moving it aside to a `.old` sibling, renaming the staged
/// copy into place, and rolling back to the original if that final rename fails.
///
/// This is how the windows [`swap_binary`] works — windows cannot rename over a
/// running `.exe`. Factored out and compiled under `test` as well (not just
/// `windows`) so its move-aside / rollback logic is actually executed by the
/// unix CI test lane instead of only cross-compiled.
#[cfg(any(windows, test))]
fn swap_via_rename_aside(target: &Path, source: &Path) -> Result<()> {
    let staged = sibling_temp(target);
    stage_copy(source, &staged)?;

    let old = target.with_extension("old");
    let _ = fs::remove_file(&old); // clear a stale `.old` from a prior update
    fs::rename(target, &old).map_err(|e| {
        let _ = fs::remove_file(&staged);
        anyhow::Error::new(e).context(format!("moving {} aside", target.display()))
    })?;
    if let Err(e) = fs::rename(&staged, target) {
        let _ = fs::rename(&old, target); // roll back to the original binary
        let _ = fs::remove_file(&staged);
        return Err(anyhow::Error::new(e).context(format!("replacing {}", target.display())));
    }
    // ponytail: the just-moved `.old` may still be locked by the running
    // process; the remove is best-effort and a later run clears it. Add the
    // MoveFileEx-on-reboot dance only if `.old` files ever prove to accumulate.
    let _ = fs::remove_file(&old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_for_finds_the_matching_line_and_ignores_others() {
        let sums = b"aaaa  cuvm-1.0.0-linux-arm64.tar.gz\n\
                     bbbb  cuvm-1.0.0-linux-amd64.tar.gz\n\
                     cccc  SHA256SUMS.sig\n";
        assert_eq!(
            sha_for(sums, "cuvm-1.0.0-linux-amd64.tar.gz").unwrap(),
            "bbbb"
        );
    }

    #[test]
    fn sha_for_tolerates_a_binary_mode_star_prefix() {
        let sums = b"dead  *cuvm-1.0.0-windows-amd64.zip\n";
        assert_eq!(
            sha_for(sums, "cuvm-1.0.0-windows-amd64.zip").unwrap(),
            "dead"
        );
    }

    #[test]
    fn sha_for_errors_when_the_archive_is_absent() {
        let sums = b"aaaa  some-other-file\n";
        assert!(sha_for(sums, "cuvm-1.0.0-linux-amd64.tar.gz").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn swap_binary_replaces_content_sets_exec_bit_and_leaves_no_temp() {
        use std::os::unix::fs::PermissionsExt;
        let dir = assert_fs::TempDir::new().unwrap();
        let target = dir.path().join("bin/cuvm");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"OLD-BINARY").unwrap();
        let source = dir.path().join("staged/cuvm");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"NEW-AND-LONGER-BINARY").unwrap();

        swap_binary(&target, &source).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"NEW-AND-LONGER-BINARY");
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o111,
            0o111,
            "exec bits must be set on the new binary"
        );

        let leftovers: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".new."))
            .collect();
        assert!(leftovers.is_empty(), "staged temp leaked: {leftovers:?}");
    }

    #[cfg(unix)]
    #[test]
    fn swap_binary_leaves_target_intact_when_source_is_missing() {
        let dir = assert_fs::TempDir::new().unwrap();
        let target = dir.path().join("cuvm");
        fs::write(&target, b"PRECIOUS").unwrap();

        let err = swap_binary(&target, &dir.path().join("does-not-exist")).unwrap_err();
        assert!(format!("{err:#}").contains("staging"), "{err:#}");
        assert_eq!(
            fs::read(&target).unwrap(),
            b"PRECIOUS",
            "target must survive"
        );
    }

    // The windows swap goes through `swap_via_rename_aside`; these exercise its
    // success and move-aside-failure paths on the (unix) CI lane, where a
    // windows-cfg test would never run.

    #[test]
    fn rename_aside_replaces_content_and_leaves_no_old_or_new_leftovers() {
        let dir = assert_fs::TempDir::new().unwrap();
        let target = dir.path().join("bin/cuvm.exe");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"OLD").unwrap();
        let source = dir.path().join("staged");
        fs::write(&source, b"NEW-PAYLOAD").unwrap();

        swap_via_rename_aside(&target, &source).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"NEW-PAYLOAD");
        // Neither the `.old` sideline nor a `.new.` temp may survive a success.
        let leftovers: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".new.") || n.contains(".old"))
            .collect();
        assert!(leftovers.is_empty(), "swap leftovers: {leftovers:?}");
    }

    #[test]
    fn rename_aside_preserves_the_target_when_the_move_aside_fails() {
        // Squat a non-empty directory on the `.old` path so `rename(target → old)`
        // fails after the copy is staged: the original binary must survive and the
        // staged temp must be cleaned up (no brick, no leak on a failed swap).
        let dir = assert_fs::TempDir::new().unwrap();
        let target = dir.path().join("cuvm.exe");
        fs::write(&target, b"ORIGINAL").unwrap();
        let source = dir.path().join("staged");
        fs::write(&source, b"NEW-PAYLOAD").unwrap();

        let blocked_old = target.with_extension("old");
        fs::create_dir(&blocked_old).unwrap();
        fs::write(blocked_old.join("occupant"), b"x").unwrap();

        let err = swap_via_rename_aside(&target, &source).unwrap_err();
        assert!(format!("{err:#}").contains("aside"), "{err:#}");
        assert_eq!(
            fs::read(&target).unwrap(),
            b"ORIGINAL",
            "the original binary must survive a failed move-aside"
        );
        let leaked: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".new."))
            .collect();
        assert!(leaked.is_empty(), "staged temp leaked: {leaked:?}");
    }
}
