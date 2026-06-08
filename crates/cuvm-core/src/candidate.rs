//! Pure description of a toolkit directory discovered on disk but not yet adopted.
//! Zero I/O — the actual filesystem walk + validation lives in `cuvm-platform`.

use std::path::PathBuf;

use crate::domain::Platform;
use crate::version::Version;

/// A discovered, validated-on-the-platform-side toolkit directory awaiting adoption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Parsed toolkit version (from the dir name for `/usr/local/cuda-X.Y`).
    pub version: Version,
    /// Absolute path to the toolkit root, recorded verbatim (adopted in place).
    pub root: PathBuf,
    /// Target platform of the host doing the adoption.
    pub platform: Platform,
}

impl Candidate {
    /// Stable handle used as the manifest key for this candidate (== version raw).
    #[must_use]
    pub fn handle(&self) -> String {
        self.version.raw.clone()
    }

    /// Parse a `cuda-X.Y[.Z]` directory *name* into a [`Candidate`].
    ///
    /// Returns `None` if `name` does not match the `cuda-<version>` shape or the
    /// version part fails to parse. The bare `cuda` symlink name returns `None`
    /// here on purpose — the symlink is resolved to its target dir before this is
    /// called (see `cuvm-platform`'s scan).
    #[must_use]
    pub fn from_dir_name(name: &str, root: PathBuf, platform: Platform) -> Option<Candidate> {
        let rest = name.strip_prefix("cuda-")?;
        if rest.is_empty() {
            return None;
        }
        let version = Version::parse(rest).ok()?;
        Some(Candidate {
            version,
            root,
            platform,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Arch, Os, Platform};
    use crate::version::Version;
    use std::path::PathBuf;

    fn linux() -> Platform {
        Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        }
    }

    #[test]
    fn from_dir_name_parses_minor_version() {
        let c =
            Candidate::from_dir_name("cuda-12.4", PathBuf::from("/usr/local/cuda-12.4"), linux())
                .expect("cuda-12.4 should parse");
        assert_eq!(c.version, Version::parse("12.4").unwrap());
        assert_eq!(c.root, PathBuf::from("/usr/local/cuda-12.4"));
        assert_eq!(c.handle(), "12.4");
    }

    #[test]
    fn from_dir_name_parses_patch_version() {
        let c = Candidate::from_dir_name("cuda-12.4.1", PathBuf::from("/x/cuda-12.4.1"), linux())
            .expect("cuda-12.4.1 should parse");
        assert_eq!(c.version, Version::parse("12.4.1").unwrap());
    }

    #[test]
    fn from_dir_name_rejects_non_cuda_dirs() {
        assert!(
            Candidate::from_dir_name("cuda", PathBuf::from("/usr/local/cuda"), linux()).is_none()
        );
        assert!(Candidate::from_dir_name("cudnn-9.2", PathBuf::from("/x"), linux()).is_none());
        assert!(Candidate::from_dir_name("cuda-", PathBuf::from("/x"), linux()).is_none());
        assert!(Candidate::from_dir_name("cuda-banana", PathBuf::from("/x"), linux()).is_none());
        assert!(Candidate::from_dir_name("notcuda-12.4", PathBuf::from("/x"), linux()).is_none());
    }
}
