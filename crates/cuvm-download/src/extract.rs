//! Archive extraction for redist toolkit components.
//!
//! `extract_tar_xz` uses a pure-Rust xz decode (`lzma-rs`) so the musl build stays
//! fully static (no C `liblzma`), then unpacks the inner tar. `extract_zip` uses the
//! `zip` crate for Windows redistributables. Both route every entry through
//! [`safe_join`], a zip-slip guard that rejects `..` traversal and absolute paths.

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
// Consumed by `extract_tar_xz`/`extract_zip` in the following tasks; until then it
// is reached only from tests, so the lib-only build sees it as unused.
#[cfg_attr(not(test), allow(dead_code))]
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
