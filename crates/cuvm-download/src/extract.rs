//! Archive extraction for redist toolkit components.
//!
//! `extract_tar_xz` uses a pure-Rust xz decode (`lzma-rs`) so the musl build stays
//! fully static (no C `liblzma`), then unpacks the inner tar. `extract_zip` uses the
//! `zip` crate for Windows redistributables. Both route every entry through
//! [`safe_join`], a zip-slip guard that rejects `..` traversal and absolute paths.

use std::fs;
use std::io::{copy, Cursor, Read};
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Errors raised while decoding or unpacking an archive.
#[derive(Debug, Error)]
pub enum ExtractError {
    /// An archive entry, once normalized, would land outside the destination
    /// directory (a `..` traversal or an absolute path). Nothing is written.
    #[error("archive entry `{entry}` escapes the destination directory")]
    ZipSlip {
        /// The offending entry path, verbatim as stored in the archive.
        entry: String,
    },

    /// The xz/lzma stream could not be decoded.
    #[error("xz decode failed: {0}")]
    Xz(String),

    /// A zip archive could not be opened or read.
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// Underlying filesystem / tar I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// `strip_wrapper_dir` found a shape it cannot safely flatten.
    #[error("cannot strip wrapper dir: {0}")]
    Wrapper(String),
}

/// Join `entry` onto `dest`, rejecting any path that escapes `dest`.
///
/// Normalizes `.` and `..` lexically (no filesystem access): an absolute entry,
/// a root/prefix component, or any `..` that pops above `dest` yields
/// [`ExtractError::ZipSlip`]. This is the single zip-slip guard shared by both
/// `extract_tar_xz` and `extract_zip`.
///
/// # Errors
/// Returns [`ExtractError::ZipSlip`] if the normalized entry would escape `dest`.
fn safe_join(dest: &Path, entry: &str) -> Result<PathBuf, ExtractError> {
    let slip = || ExtractError::ZipSlip {
        entry: entry.to_string(),
    };
    let rel = Path::new(entry);
    let mut out = PathBuf::from(dest);
    let mut depth: usize = 0;
    for comp in rel.components() {
        match comp {
            Component::Normal(c) => {
                out.push(c);
                depth += 1;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return Err(slip());
                }
                depth -= 1;
                out.pop();
            }
            // Absolute root or a Windows drive/UNC prefix: reject outright.
            Component::RootDir | Component::Prefix(_) => return Err(slip()),
        }
    }
    Ok(out)
}

/// Decode a `.tar.xz` (pure-Rust xz via `lzma-rs`) and unpack it into `dest`.
///
/// Every entry is routed through [`safe_join`] so a malicious archive cannot
/// escape `dest`. `dest` is created (recursively) if it does not exist.
///
/// # Errors
/// Returns [`ExtractError::Xz`] on xz-decode failure, [`ExtractError::ZipSlip`]
/// if any entry escapes `dest`, or [`ExtractError::Io`] on filesystem failure.
pub fn extract_tar_xz(archive: &Path, dest: &Path) -> Result<(), ExtractError> {
    let raw = fs::read(archive)?;
    let mut decoded = Vec::new();
    lzma_rs::xz_decompress(&mut Cursor::new(raw), &mut decoded)
        .map_err(|e| ExtractError::Xz(e.to_string()))?;

    fs::create_dir_all(dest)?;
    let mut tar = tar::Archive::new(Cursor::new(decoded));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let rel = path.to_string_lossy().into_owned();
        let target = safe_join(dest, &rel)?;

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        fs::write(&target, &bytes)?;
    }
    Ok(())
}

/// Decode a `.tar.gz` (pure-Rust gzip via `flate2`/`miniz_oxide`) and unpack it
/// into `dest`. The gzip sibling of [`extract_tar_xz`]: same [`safe_join`]
/// zip-slip guard, same directory creation. Our release archives ship as
/// `.tar.gz` (see `install.sh`), so `cuvm self update` reads its own release
/// through this path.
///
/// # Errors
/// Returns [`ExtractError::Io`] on a gzip-decode or filesystem failure, or
/// [`ExtractError::ZipSlip`] if any entry escapes `dest`.
pub fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), ExtractError> {
    let file = fs::File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);

    fs::create_dir_all(dest)?;
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let rel = path.to_string_lossy().into_owned();
        let target = safe_join(dest, &rel)?;

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        fs::write(&target, &bytes)?;
    }
    Ok(())
}

/// Unpack a `.zip` into `dest` using the `zip` crate.
///
/// Every entry is routed through [`safe_join`] (not the archive's own
/// `enclosed_name`), so the same zip-slip guard that protects the tar path also
/// rejects a `..` traversal or absolute entry here. `dest` is created
/// (recursively) if it does not exist.
///
/// # Errors
/// Returns [`ExtractError::Zip`] on a malformed archive, [`ExtractError::ZipSlip`]
/// if any entry escapes `dest`, or [`ExtractError::Io`] on filesystem failure.
pub fn extract_zip(archive: &Path, dest: &Path) -> Result<(), ExtractError> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    fs::create_dir_all(dest)?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let raw_name = entry.name().to_string();
        // Re-run the name through our own guard (do not trust the archive).
        let target = safe_join(dest, &raw_name)?;

        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&target)?;
        copy(&mut entry, &mut out)?;
    }
    Ok(())
}

/// Flatten a single redist wrapper directory in `dir`, one level deep.
///
/// Redist tarballs wrap their tree in exactly one top-level directory
/// (`"<component>-<platform>-<version>-archive/"`). When `dir`'s top level holds
/// exactly one entry, a directory, and no files, this moves that directory's
/// children up into `dir` and removes the now-empty wrapper. Any other shape
/// (already flat, multiple entries, or a stray top-level file) is a safe no-op.
///
/// # Errors
/// Returns [`ExtractError::Io`] on a filesystem failure, or
/// [`ExtractError::Wrapper`] if a child cannot be moved up (e.g. a name clash).
pub fn strip_wrapper_dir(dir: &Path) -> Result<(), ExtractError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        entries.push((entry.path(), entry.file_type()?));
    }

    // Only flatten the unambiguous single-wrapper case.
    if entries.len() != 1 {
        return Ok(());
    }
    let (wrapper, file_type) = &entries[0];
    if !file_type.is_dir() {
        return Ok(());
    }

    // Move every child of the wrapper up into `dir`.
    for child in fs::read_dir(wrapper)? {
        let child = child?;
        let from = child.path();
        let name = child.file_name();
        let to = dir.join(&name);
        if to.exists() {
            return Err(ExtractError::Wrapper(format!(
                "name clash flattening {}: {} already exists",
                wrapper.display(),
                to.display()
            )));
        }
        fs::rename(&from, &to)?;
    }

    fs::remove_dir(wrapper)?;
    Ok(())
}

#[cfg(test)]
mod tar_xz_tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

    /// Build a `.tar.xz` in memory from `(path, bytes)` entries, write it to `at`.
    #[allow(clippy::cast_possible_truncation)]
    fn write_tar_xz(at: &Path, entries: &[(&str, &[u8])]) {
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            for (name, data) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, Cursor::new(*data))
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        let mut xz_buf = Vec::new();
        lzma_rs::xz_compress(&mut Cursor::new(&tar_buf), &mut xz_buf).unwrap();
        std::fs::write(at, &xz_buf).unwrap();
    }

    #[test]
    fn extracts_files_with_contents() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("comp.tar.xz");
        write_tar_xz(
            &archive,
            &[
                ("bin/nvcc", b"#!fake-nvcc"),
                ("lib/libcudart.so", b"ELF-ish"),
            ],
        );

        let dest = dir.path().join("out");
        extract_tar_xz(&archive, &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("bin/nvcc")).unwrap(),
            b"#!fake-nvcc"
        );
        assert_eq!(
            std::fs::read(dest.join("lib/libcudart.so")).unwrap(),
            b"ELF-ish"
        );
    }

    #[test]
    fn creates_dest_when_missing() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("comp.tar.xz");
        write_tar_xz(&archive, &[("include/cuda.h", b"#pragma once")]);

        let dest = dir.path().join("nested/does/not/exist");
        extract_tar_xz(&archive, &dest).unwrap();

        assert!(dest.join("include/cuda.h").is_file());
    }
}

#[cfg(test)]
mod tar_gz_tests {
    use super::*;
    use std::io::{Cursor, Write};
    use tempfile::tempdir;

    /// Build a `.tar.gz` in memory from `(path, bytes)` entries, write it to `at`.
    #[allow(clippy::cast_possible_truncation)]
    fn write_tar_gz(at: &Path, entries: &[(&str, &[u8])]) {
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            for (name, data) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, Cursor::new(*data))
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        let file = fs::File::create(at).unwrap();
        let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        enc.write_all(&tar_buf).unwrap();
        enc.finish().unwrap();
    }

    #[test]
    fn extracts_the_release_binary_from_a_tar_gz() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("cuvm-1.0.0-linux-amd64.tar.gz");
        write_tar_gz(
            &archive,
            &[
                ("cuvm-1.0.0-linux-amd64/cuvm", b"#!/bin/sh\necho hi\n"),
                ("cuvm-1.0.0-linux-amd64/shims/cuvm.sh", b"# shim\n"),
            ],
        );

        let dest = dir.path().join("out");
        extract_tar_gz(&archive, &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("cuvm-1.0.0-linux-amd64/cuvm")).unwrap(),
            b"#!/bin/sh\necho hi\n"
        );
        assert!(dest.join("cuvm-1.0.0-linux-amd64/shims/cuvm.sh").is_file());
    }

    #[test]
    fn rejects_parent_traversal() {
        // A `..` entry must be refused by the shared safe_join guard, exactly as
        // for the tar.xz path — no file lands outside `dest`.
        let dir = tempdir().unwrap();
        let archive = dir.path().join("evil.tar.gz");
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let mut header = tar::Header::new_gnu();
            header.set_size(5);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header
                .as_gnu_mut()
                .unwrap()
                .name
                .get_mut(..13)
                .unwrap()
                .copy_from_slice(b"../escape.txt");
            header.set_cksum();
            builder.append(&header, Cursor::new(&b"pwned"[..])).unwrap();
            builder.finish().unwrap();
        }
        let file = fs::File::create(&archive).unwrap();
        let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        enc.write_all(&tar_buf).unwrap();
        enc.finish().unwrap();

        let dest = dir.path().join("out");
        let err = extract_tar_gz(&archive, &dest).unwrap_err();
        assert!(matches!(err, ExtractError::ZipSlip { .. }), "{err:?}");
        assert!(!dir.path().join("escape.txt").exists());
    }
}

#[cfg(test)]
mod zip_tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    /// Build a `.zip` at `at` from `(path, bytes)` entries.
    fn write_zip(at: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(at).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn extracts_files_with_contents() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("comp.zip");
        write_zip(
            &archive,
            &[
                ("bin/nvcc.exe", b"MZ-fake"),
                ("lib/x64/cudart.lib", b"libdata"),
            ],
        );

        let dest = dir.path().join("out");
        extract_zip(&archive, &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("bin/nvcc.exe")).unwrap(),
            b"MZ-fake"
        );
        assert_eq!(
            std::fs::read(dest.join("lib/x64/cudart.lib")).unwrap(),
            b"libdata"
        );
    }

    #[test]
    fn creates_dest_when_missing() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("comp.zip");
        write_zip(&archive, &[("include/cuda.h", b"#pragma once")]);

        let dest = dir.path().join("nested/missing");
        extract_zip(&archive, &dest).unwrap();

        assert!(dest.join("include/cuda.h").is_file());
    }
}

#[cfg(test)]
mod zip_slip_e2e_tests {
    use super::*;
    use std::io::{Cursor, Write};
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    /// Write the raw bytes of `name` into a GNU tar header's name field,
    /// bypassing `set_path`'s own `..` rejection so the stored entry carries a
    /// literal traversal path — the attack our guard must defeat.
    fn set_raw_name(header: &mut tar::Header, name: &[u8]) {
        let gnu = header.as_gnu_mut().expect("gnu header");
        gnu.name[..name.len()].copy_from_slice(name);
    }

    fn malicious_tar_xz(at: &Path) {
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let data = b"pwned";
            let mut header = tar::Header::new_gnu();
            #[allow(clippy::cast_possible_truncation)]
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            // The `tar` crate's `set_path`/`append_data` refuse a `..` path, so we
            // write the traversal name straight into the header bytes, then append
            // the prebuilt header (which does not re-validate the path).
            set_raw_name(&mut header, b"../escape.txt");
            header.set_cksum();
            builder.append(&header, Cursor::new(&data[..])).unwrap();
            builder.finish().unwrap();
        }
        let mut xz_buf = Vec::new();
        lzma_rs::xz_compress(&mut Cursor::new(&tar_buf), &mut xz_buf).unwrap();
        std::fs::write(at, &xz_buf).unwrap();
    }

    fn malicious_zip(at: &Path) {
        let file = std::fs::File::create(at).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        // Preserve the literal "../escape.txt" instead of letting the writer
        // sanitize it, so the stored name is the traversal the guard must defeat.
        zw.start_file("../escape.txt", opts).unwrap();
        zw.write_all(b"pwned").unwrap();
        zw.finish().unwrap();
    }

    #[test]
    fn tar_xz_rejects_parent_traversal_and_writes_nothing_outside() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("evil.tar.xz");
        malicious_tar_xz(&archive);

        // Prove the traversal name survived into the archive (the tar reader
        // surfaces it verbatim), so the guard — not a malformed-archive error —
        // is what rejects it.
        {
            let raw = std::fs::read(&archive).unwrap();
            let mut decoded = Vec::new();
            lzma_rs::xz_decompress(&mut Cursor::new(raw), &mut decoded).unwrap();
            let mut tar = tar::Archive::new(Cursor::new(decoded));
            let first = tar.entries().unwrap().next().unwrap().unwrap();
            assert_eq!(first.path().unwrap().to_string_lossy(), "../escape.txt");
        }

        let dest = dir.path().join("out");
        let err = extract_tar_xz(&archive, &dest).unwrap_err();
        assert!(matches!(err, ExtractError::ZipSlip { .. }), "{err:?}");

        // The escaped sibling of `dest` must not exist.
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    fn zip_rejects_parent_traversal_and_writes_nothing_outside() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("evil.zip");
        malicious_zip(&archive);

        // Prove the malicious name survived into the archive (the writer did not
        // sanitize it), so the guard — not the writer — is what rejects it.
        {
            let f = std::fs::File::open(&archive).unwrap();
            let mut z = zip::ZipArchive::new(f).unwrap();
            assert_eq!(z.by_index(0).unwrap().name(), "../escape.txt");
        }

        let dest = dir.path().join("out");
        let err = extract_zip(&archive, &dest).unwrap_err();
        assert!(matches!(err, ExtractError::ZipSlip { .. }), "{err:?}");

        assert!(!dir.path().join("escape.txt").exists());
    }
}

#[cfg(test)]
mod strip_wrapper_tests {
    use super::*;
    use tempfile::tempdir;

    fn touch(p: &Path, body: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn flattens_single_wrapper_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let wrapper = root.join("cuda_nvcc-linux-x86_64-12.4.131-archive");
        touch(&wrapper.join("bin/nvcc"), b"x");
        touch(&wrapper.join("lib/libcudart.so"), b"y");
        touch(&wrapper.join("LICENSE"), b"lic");

        strip_wrapper_dir(root).unwrap();

        assert!(root.join("bin/nvcc").is_file());
        assert!(root.join("lib/libcudart.so").is_file());
        assert!(root.join("LICENSE").is_file());
        // The wrapper itself is gone.
        assert!(!wrapper.exists());
    }

    #[test]
    fn already_flat_is_noop() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("bin/nvcc"), b"x");
        touch(&root.join("include/cuda.h"), b"h");

        strip_wrapper_dir(root).unwrap();

        assert!(root.join("bin/nvcc").is_file());
        assert!(root.join("include/cuda.h").is_file());
    }

    #[test]
    fn top_level_file_beside_dir_is_noop() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let wrapper = root.join("only-archive");
        touch(&wrapper.join("bin/nvcc"), b"x");
        touch(&root.join("stray.txt"), b"s"); // a file sits at top level

        strip_wrapper_dir(root).unwrap();

        // Not flattened: the wrapper dir survives untouched.
        assert!(wrapper.join("bin/nvcc").is_file());
        assert!(root.join("stray.txt").is_file());
    }

    #[test]
    fn empty_dir_is_noop() {
        let dir = tempdir().unwrap();
        strip_wrapper_dir(dir.path()).unwrap();
        // Nothing created, nothing removed.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn zip_slip_error_names_the_offending_entry() {
        let e = ExtractError::ZipSlip {
            entry: "../escape.txt".to_string(),
        };
        let msg = e.to_string();
        assert!(msg.contains("../escape.txt"), "{msg}");
        assert!(msg.to_lowercase().contains("escapes"), "{msg}");
    }

    #[test]
    fn safe_join_rejects_parent_traversal() {
        let dest = Path::new("/dest");
        let err = safe_join(dest, "a/../../etc/passwd").unwrap_err();
        assert!(matches!(err, ExtractError::ZipSlip { .. }));
    }

    #[test]
    fn safe_join_rejects_absolute_entry() {
        let dest = Path::new("/dest");
        let err = safe_join(dest, "/etc/passwd").unwrap_err();
        assert!(matches!(err, ExtractError::ZipSlip { .. }));
    }

    #[test]
    fn safe_join_allows_normal_nested_path() {
        let dest = Path::new("/dest");
        let joined = safe_join(dest, "bin/nvcc").unwrap();
        assert_eq!(joined, Path::new("/dest/bin/nvcc"));
    }

    #[test]
    fn safe_join_normalizes_interior_curdir_and_keeps_inside() {
        let dest = Path::new("/dest");
        let joined = safe_join(dest, "lib/./libcudart.so").unwrap();
        assert_eq!(joined, Path::new("/dest/lib/libcudart.so"));
    }
}
