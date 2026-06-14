use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Sbsa,
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

/// Returns the `Platform` for the current compilation target.
#[must_use]
pub fn current_platform() -> Platform {
    Platform {
        os: if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        },
        arch: if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else {
            // sbsa is also aarch64-based; fall back to X86_64 for unknown targets
            Arch::X86_64
        },
    }
}

impl Platform {
    /// The redist platform-directory key, e.g. `"linux-x86_64"`.
    #[must_use]
    pub fn redist_key(&self) -> String {
        let os = match self.os {
            Os::Linux => "linux",
            Os::Windows => "windows",
        };
        let arch = match self.arch {
            Arch::X86_64 => "x86_64",
            Arch::Sbsa => "sbsa",
            Arch::Aarch64 => "aarch64",
        };
        format!("{os}-{arch}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    PowerShell,
    Cmd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Adopted,
    Downloaded,
    Supplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuClass {
    Unknown,
    GeForce,
    DataCenter,
    Jetson,
    NgcReadyRtx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolkit {
    pub version: Version,
    pub source: Source,
    pub root: PathBuf,
    pub platform: Platform,
    pub components: Vec<String>,
    pub has_lib64: bool,
    pub installed_at: OffsetDateTime,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cudnn {
    pub version: Version,
    pub cuda_major: u32,
    pub source: Source,
    pub store: PathBuf,
    pub sha256: String,
    pub libs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Companion {
    pub name: String,
    pub version: Version,
    pub store: PathBuf,
    pub sha256: String,
}

/// The optional CUDA math-library components, requestable at install time via
/// `cuvm install <spec> --with <comp,…>` (spec §2.1, "Recommended set + math
/// libs on request"). They ship in the SAME toolkit `redistrib_<ver>.json` as the
/// recommended set — each carrying its own sha256 — so they ride the normal
/// resolve→verify→extract→place pipeline into the toolkit root and are surfaced
/// read-only as [`Bundle::extra`] companions afterward.
pub const MATH_LIB_COMPONENTS: &[&str] = &[
    "libcublas",
    "libcufft",
    "libcurand",
    "libcusolver",
    "libcusparse",
    "libnpp",
    "libnvjitlink",
];

/// True when `component` is one of the [`MATH_LIB_COMPONENTS`] math libs.
#[must_use]
pub fn is_math_lib(component: &str) -> bool {
    MATH_LIB_COMPONENTS.contains(&component)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub toolkit: Toolkit,
    pub cudnn: Option<Cudnn>,
    pub extra: Vec<Companion>,
}

impl Bundle {
    /// The stable handle for a bundle == the toolkit version's raw string.
    #[must_use]
    pub fn handle(&self) -> String {
        self.toolkit.version.raw.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub spec: String,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    pub present: bool,
    pub version: Version,
    pub platform: Platform,
    pub gpu_class: GpuClass,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redist_key_matches_redist_platform_dirs() {
        assert_eq!(
            Platform {
                os: Os::Linux,
                arch: Arch::X86_64
            }
            .redist_key(),
            "linux-x86_64"
        );
        assert_eq!(
            Platform {
                os: Os::Linux,
                arch: Arch::Sbsa
            }
            .redist_key(),
            "linux-sbsa"
        );
        assert_eq!(
            Platform {
                os: Os::Linux,
                arch: Arch::Aarch64
            }
            .redist_key(),
            "linux-aarch64"
        );
        assert_eq!(
            Platform {
                os: Os::Windows,
                arch: Arch::X86_64
            }
            .redist_key(),
            "windows-x86_64"
        );
    }

    #[test]
    fn bundle_handle_equals_toolkit_version_raw() {
        let tk = Toolkit {
            version: crate::Version::parse("12.4.1").unwrap(),
            source: Source::Downloaded,
            root: std::path::PathBuf::from("/home/u/.cuvm/versions/12.4.1"),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
            has_lib64: false,
            installed_at: time::OffsetDateTime::UNIX_EPOCH,
            checksum: None,
        };
        let b = Bundle {
            toolkit: tk,
            cudnn: None,
            extra: vec![],
        };
        assert_eq!(b.handle(), "12.4.1");
    }

    #[test]
    fn source_serde_round_trips_lowercase() {
        let json = serde_json::to_string(&Source::Adopted).unwrap();
        assert_eq!(json, "\"adopted\"");
        let back: Source = serde_json::from_str("\"downloaded\"").unwrap();
        assert!(matches!(back, Source::Downloaded));
    }
}
