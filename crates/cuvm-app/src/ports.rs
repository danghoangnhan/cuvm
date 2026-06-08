use std::path::PathBuf;

use cuvm_core::{Bundle, Pin};

// ----- Resolver outputs -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub bundle: Bundle,
    pub spec: String,
    pub via: ResolveVia,
    pub pin: Option<Pin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveVia {
    Exact,
    Minor,
    Major,
    Latest,
    Alias,
    PinFile,
    Default,
}

// ----- Compat engine outputs -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub ok: bool,
    pub severity: Severity,
    pub reason: String,
    pub forward_compat_possible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Ok,
    Warn,
    Block,
}

// ----- Registry outputs -----

/// Mirrors one redist platform object; `relative_path` is taken verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub component: String,
    pub relative_path: String,
    pub url: String,
    pub sha256: String,
    pub md5: Option<String>,
    pub size: u64,
}

// ----- Installer inputs/outputs (fields expanded in their owning WUs) -----

/// What to acquire for an install. Fields land in WU-10/WU-13/WU-14.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquirePlan {
    pub artifacts: Vec<Artifact>,
    pub dest_handle: String,
}

/// A downloaded, on-disk artifact. Fields land in WU-11/WU-12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cached {
    pub artifact: Artifact,
    pub path: PathBuf,
}

/// Kind of user-supplied artifact for `ingest_supplied`. Expanded in WU-17.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Toolkit,
    Cudnn,
}

/// A scan candidate (existing on-disk install). Fields land in WU-4/WU-9.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub root: PathBuf,
    pub version_hint: Option<String>,
}

/// Which components to request from the registry. Expanded in WU-10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentPolicy {
    /// The minimal usable set (manifest-driven, version-branched).
    Recommended,
    /// An explicit component-name allowlist.
    Only(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::{Arch, Bundle, Os, Platform, Source, Toolkit, Version};

    fn bundle() -> Bundle {
        Bundle {
            toolkit: Toolkit {
                version: Version::parse("12.4.1").unwrap(),
                source: Source::Downloaded,
                root: "/p".into(),
                platform: Platform {
                    os: Os::Linux,
                    arch: Arch::X86_64,
                },
                components: vec![],
                has_lib64: false,
                installed_at: time::OffsetDateTime::UNIX_EPOCH,
                checksum: None,
            },
            cudnn: None,
            extra: vec![],
        }
    }

    #[test]
    fn resolved_carries_bundle_spec_via_and_pin() {
        let r = Resolved {
            bundle: bundle(),
            spec: "12.4".to_string(),
            via: ResolveVia::Minor,
            pin: None,
        };
        assert_eq!(r.spec, "12.4");
        assert!(matches!(r.via, ResolveVia::Minor));
        assert!(r.pin.is_none());
    }

    #[test]
    fn verdict_blocks_with_reason() {
        let v = Verdict {
            ok: false,
            severity: Severity::Block,
            reason: "driver ceiling exceeded".to_string(),
            forward_compat_possible: false,
        };
        assert!(!v.ok);
        assert!(matches!(v.severity, Severity::Block));
        assert_eq!(v.reason, "driver ceiling exceeded");
    }

    #[test]
    fn artifact_mirrors_one_redist_platform_object() {
        let a = Artifact {
            component: "cuda_nvcc".to_string(),
            relative_path: "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-12.4.131-archive.tar.xz"
                .to_string(),
            url: "https://developer.download.nvidia.com/compute/cuda/redist/...".to_string(),
            sha256: "deadbeef".to_string(),
            md5: None,
            size: 1234,
        };
        assert_eq!(a.component, "cuda_nvcc");
        assert!(a.relative_path.starts_with("cuda_nvcc/"));
        assert_eq!(a.size, 1234);
    }
}
