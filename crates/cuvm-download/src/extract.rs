//! Archive extraction for redist toolkit components.
//!
//! `extract_tar_xz` uses a pure-Rust xz decode (`lzma-rs`) so the musl build stays
//! fully static (no C `liblzma`), then unpacks the inner tar. `extract_zip` uses the
//! `zip` crate for Windows redistributables. Both route every entry through
//! [`safe_join`], a zip-slip guard that rejects `..` traversal and absolute paths.

use std::fs;
use std::io::{Cursor, Read};
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
