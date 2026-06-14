//! Link a content-addressed companion payload (cuDNN, NCCL) into a DOWNLOADED
//! toolkit tree (spec §2.3/§10: full lib set + headers; symlink Unix, copy
//! Windows). Adopted toolkits are never modified — callers enforce that before
//! reaching this module (plan D8). The core link/unlink logic is generic over a
//! payload-name `needle`; `link_cudnn`/`link_nccl` are thin wrappers.

use std::path::Path;

use anyhow::{Context, Result};
use cuvm_core::Os;

/// Subdir pairs scanned in the store and mirrored into the toolkit root.
/// `lib` covers Linux .so sets and Windows import libs; `bin` covers Windows
/// DLLs; `include` covers headers. Missing store subdirs are skipped.
const SUBDIRS: [&str; 3] = ["lib", "bin", "include"];

/// Link the full cuDNN payload (entries whose name contains `cudnn`).
///
/// # Errors
/// Filesystem failures (creating dirs, reading the store, symlinking, copying).
pub fn link_cudnn(os: Os, store: &Path, toolkit_root: &Path) -> Result<Vec<String>> {
    link_named(os, store, toolkit_root, "cudnn")
}

/// Remove previously linked cuDNN entries from the toolkit.
///
/// # Errors
/// Filesystem failures while removing entries.
pub fn unlink_cudnn(os: Os, toolkit_root: &Path) -> Result<()> {
    unlink_named(os, toolkit_root, "cudnn")
}

/// Link the full NCCL payload (entries whose name contains `nccl`).
///
/// # Errors
/// Filesystem failures (creating dirs, reading the store, symlinking, copying).
pub fn link_nccl(os: Os, store: &Path, toolkit_root: &Path) -> Result<Vec<String>> {
    link_named(os, store, toolkit_root, "nccl")
}

/// Remove previously linked NCCL entries from the toolkit.
///
/// # Errors
/// Filesystem failures while removing entries.
pub fn unlink_nccl(os: Os, toolkit_root: &Path) -> Result<()> {
    unlink_named(os, toolkit_root, "nccl")
}

/// Link (Unix: absolute symlinks) or copy (Windows) every `needle`-named entry
/// from the store's `lib`/`bin`/`include` into the same subdirs of
/// `toolkit_root`. On Windows, directory entries under `lib`/`bin` (e.g.
/// `lib/x64/`) are copied recursively regardless of their own name. Returns the
/// linked library file names — top-level `needle`-named files from `lib` +
/// `bin` — sorted: the authoritative `libs` record. Existing same-named entries
/// are replaced. (The store-side `lib_names` helpers, which also count
/// `needle`-named directories, are only an approximation of this.)
///
/// # Errors
/// Filesystem failures (creating dirs, reading the store, symlinking, copying).
pub fn link_named(os: Os, store: &Path, toolkit_root: &Path, needle: &str) -> Result<Vec<String>> {
    let mut libs: Vec<String> = Vec::new();

    for sub in SUBDIRS {
        let src_dir = store.join(sub);
        if !src_dir.is_dir() {
            continue; // payload variants omit subdirs (e.g. no bin/ on linux)
        }
        let dst_dir = toolkit_root.join(sub);
        std::fs::create_dir_all(&dst_dir)
            .with_context(|| format!("creating {}", dst_dir.display()))?;

        let entries = std::fs::read_dir(&src_dir)
            .with_context(|| format!("reading store dir {}", src_dir.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("reading {}", src_dir.display()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let src = entry.path();
            let is_dir = src.is_dir();

            // Windows payloads nest import libs under lib/x64/: such directory
            // entries are copied wholesale regardless of their own name, but
            // only TOP-LEVEL payload-named FILES count toward `libs`.
            let wanted = name.contains(needle) || (os == Os::Windows && is_dir && sub != "include");
            if !wanted {
                continue;
            }
            let dst = dst_dir.join(&name);
            replace_entry(os, &src, &dst)
                .with_context(|| format!("linking {} -> {}", src.display(), dst.display()))?;
            if sub != "include" && !is_dir && name.contains(needle) {
                libs.push(name);
            }
        }
    }

    libs.sort();
    libs.dedup();
    Ok(libs)
}

/// Remove previously linked `needle`-named entries from the toolkit's `lib`/
/// `bin`/`include`. Unix removes only `needle`-named SYMLINKS (the only thing
/// linking creates, so toolkit-owned real files are safe); Windows removes
/// `needle`-named files/dirs (they were copies). Missing subdirs are skipped.
/// Known limitation: on Windows, copies nested inside non-`needle`-named
/// directories (e.g. `lib/x64/cudnn.lib`) survive unlink, because the name
/// filter only sees the top-level entry — a subsequent relink overwrites such
/// directories anyway.
///
/// # Errors
/// Filesystem failures while removing entries.
pub fn unlink_named(os: Os, toolkit_root: &Path, needle: &str) -> Result<()> {
    for sub in SUBDIRS {
        let dir = toolkit_root.join(sub);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue; // missing subdir => nothing was linked there
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.contains(needle) {
                continue;
            }
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue; // raced away; nothing left to remove
            };
            let ours = match os {
                Os::Linux => meta.file_type().is_symlink(),
                Os::Windows => true,
            };
            if !ours {
                continue;
            }
            if meta.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            }
            .with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

/// Replace `dst` with a link (`Os::Linux`) or copy (`Os::Windows`) of `src`.
fn replace_entry(os: Os, src: &Path, dst: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(dst) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(dst)?,
        Ok(_) => std::fs::remove_file(dst)?, // file or symlink (incl. to dir)
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    match os {
        Os::Linux => symlink_entry(src, dst),
        Os::Windows => copy_recursive(src, dst),
    }
}

#[cfg(unix)]
fn symlink_entry(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(not(unix))]
fn symlink_entry(_src: &Path, _dst: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "unix symlinks unavailable on this host",
    ))
}

/// Plain copy; directories (e.g. windows lib/x64/) recurse.
fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(src, dst)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(store: &Path, files: &[&str]) {
        for f in files {
            let p = store.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"payload").unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_link_symlinks_the_full_set_and_reports_libs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let root = tmp.path().join("toolkit");
        mk(
            &store,
            &["lib/libcudnn.so", "lib/libcudnn_ops.so", "include/cudnn.h"],
        );
        std::fs::create_dir_all(root.join("lib")).unwrap();

        let libs = link_cudnn(Os::Linux, &store, &root).unwrap();
        assert_eq!(libs, ["libcudnn.so", "libcudnn_ops.so"]);
        let linked = root.join("lib/libcudnn.so");
        let meta = std::fs::symlink_metadata(&linked).unwrap();
        assert!(meta.file_type().is_symlink(), "must symlink, not copy");
        assert_eq!(
            std::fs::read_link(&linked).unwrap(),
            store.join("lib/libcudnn.so")
        );
        assert!(root.join("include/cudnn.h").exists(), "headers linked too");
    }

    #[cfg(unix)]
    #[test]
    fn unix_relink_replaces_existing_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let (s1, s2) = (tmp.path().join("s1"), tmp.path().join("s2"));
        let root = tmp.path().join("toolkit");
        mk(&s1, &["lib/libcudnn.so"]);
        mk(&s2, &["lib/libcudnn.so", "lib/libcudnn_graph.so"]);
        std::fs::create_dir_all(root.join("lib")).unwrap();

        link_cudnn(Os::Linux, &s1, &root).unwrap();
        unlink_cudnn(Os::Linux, &root).unwrap();
        let libs = link_cudnn(Os::Linux, &s2, &root).unwrap();
        assert_eq!(libs, ["libcudnn.so", "libcudnn_graph.so"]);
        assert_eq!(
            std::fs::read_link(root.join("lib/libcudnn.so")).unwrap(),
            s2.join("lib/libcudnn.so")
        );
    }

    #[test]
    fn windows_link_copies_dlls_libs_and_headers() {
        // The windows arm is plain file copies — runs anywhere.
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let root = tmp.path().join("toolkit");
        mk(
            &store,
            &["bin/cudnn64_9.dll", "lib/x64/cudnn.lib", "include/cudnn.h"],
        );
        std::fs::create_dir_all(root.join("bin")).unwrap();

        let libs = link_cudnn(Os::Windows, &store, &root).unwrap();
        assert_eq!(libs, ["cudnn64_9.dll"]);
        assert!(root.join("bin/cudnn64_9.dll").is_file());
        assert!(
            root.join("lib/x64/cudnn.lib").is_file(),
            "nested lib copied"
        );
        assert!(root.join("include/cudnn.h").is_file());

        // Relinking without unlink must overwrite existing copies (fires the
        // remove-existing-dir branch of replace_entry for lib/x64).
        let relibs = link_cudnn(Os::Windows, &store, &root).unwrap();
        assert_eq!(relibs, ["cudnn64_9.dll"]);
        assert!(root.join("lib/x64/cudnn.lib").is_file());

        unlink_cudnn(Os::Windows, &root).unwrap();
        assert!(!root.join("bin/cudnn64_9.dll").exists());
        assert!(!root.join("include/cudnn.h").exists());
        assert!(
            root.join("lib/x64/cudnn.lib").is_file(),
            "known wart: nested copies inside non-cudnn-named dirs survive unlink"
        );
    }

    #[cfg(unix)]
    #[test]
    fn relink_without_unlink_replaces_existing_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let (s1, s2) = (tmp.path().join("s1"), tmp.path().join("s2"));
        let root = tmp.path().join("toolkit");
        mk(&s1, &["lib/libcudnn.so"]);
        mk(&s2, &["lib/libcudnn.so"]);

        link_cudnn(Os::Linux, &s1, &root).unwrap();
        link_cudnn(Os::Linux, &s2, &root).unwrap();
        assert_eq!(
            std::fs::read_link(root.join("lib/libcudnn.so")).unwrap(),
            s2.join("lib/libcudnn.so"),
            "second link must retarget the existing symlink without unlink"
        );
    }

    #[test]
    fn empty_store_links_nothing_and_returns_empty_libs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store"); // no lib/bin/include at all
        std::fs::create_dir_all(&store).unwrap();
        let libs = link_cudnn(Os::Windows, &store, &tmp.path().join("toolkit")).unwrap();
        assert!(libs.is_empty(), "empty store must yield Ok(empty libs)");
    }

    #[test]
    fn unlink_leaves_non_cudnn_entries_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("toolkit");
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("lib/libcudart.so"), b"toolkit-owned").unwrap();
        unlink_cudnn(Os::Windows, &root).unwrap();
        assert!(root.join("lib/libcudart.so").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn link_nccl_symlinks_only_the_nccl_set_and_unlinks_it() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let root = tmp.path().join("toolkit");
        mk(
            &store,
            &[
                "lib/libnccl.so",
                "lib/libnccl.so.2",
                "lib/libcudart.so", // a non-nccl sibling: must be ignored
                "include/nccl.h",
            ],
        );
        std::fs::create_dir_all(root.join("lib")).unwrap();

        let libs = link_nccl(Os::Linux, &store, &root).unwrap();
        assert_eq!(libs, ["libnccl.so", "libnccl.so.2"]);
        assert!(std::fs::symlink_metadata(root.join("lib/libnccl.so"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(root.join("include/nccl.h").exists(), "headers linked too");
        assert!(
            !root.join("lib/libcudart.so").exists(),
            "non-nccl entries are not linked"
        );

        unlink_nccl(Os::Linux, &root).unwrap();
        assert!(!root.join("lib/libnccl.so").exists());
        assert!(!root.join("include/nccl.h").exists());
    }
}
