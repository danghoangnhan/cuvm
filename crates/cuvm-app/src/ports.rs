use std::path::{Path, PathBuf};

use anyhow::Result;
use cuvm_core::{Bundle, Driver, Manifest, Pin, Platform, Shell, Version, VersionMeta};

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

// ----- Trait ports (object-safe; async-free; fallible ones return anyhow::Result) -----

pub trait Resolver {
    /// Resolve a version spec to a concrete bundle.
    ///
    /// # Errors
    /// Returns an error if the spec cannot be resolved to an installed bundle.
    fn resolve(&self, spec: &str) -> Result<Resolved>;

    /// Resolve from a directory's pin/alias context, if any applies.
    ///
    /// # Errors
    /// Returns an error if pin discovery or resolution fails.
    fn resolve_from_dir(&self, cwd: &Path) -> Result<Option<Resolved>>;

    /// Expand an alias name to its target spec.
    ///
    /// # Errors
    /// Returns an error if the alias does not exist or cannot be read.
    fn expand_alias(&self, name: &str) -> Result<String>;

    /// Find the nearest pin file walking upward from `cwd`.
    ///
    /// # Errors
    /// Returns an error if the filesystem walk fails.
    fn find_pin_upward(&self, cwd: &Path) -> Result<Option<cuvm_core::Pin>>;
}

pub trait Activator {
    /// Emit the activation script for a bundle in the given shell.
    ///
    /// # Errors
    /// Returns an error if the script cannot be rendered.
    fn emit_env(&self, b: &Bundle, sh: Shell) -> Result<String>;

    /// Emit the deactivation script for the given shell.
    ///
    /// # Errors
    /// Returns an error if the script cannot be rendered.
    fn emit_deactivate(&self, sh: Shell) -> Result<String>;

    /// Emit the shell-integration hook for the given shell.
    ///
    /// # Errors
    /// Returns an error if the hook cannot be rendered.
    fn hook(&self, sh: Shell) -> Result<String>;

    /// Whether this backend supports the given shell.
    fn supports(&self, sh: Shell) -> bool;
}

pub trait Installer {
    /// Download/acquire the artifacts in `plan`.
    ///
    /// # Errors
    /// Returns an error if any artifact cannot be acquired.
    fn acquire(&self, plan: &AcquirePlan) -> Result<Vec<Cached>>;

    /// Verify the checksums of cached artifacts.
    ///
    /// # Errors
    /// Returns an error if verification fails for any artifact.
    fn verify(&self, arts: &[Cached]) -> Result<()>;

    /// Extract the cached artifacts atomically into a temp area.
    ///
    /// # Errors
    /// Returns an error if extraction fails.
    fn extract_atomic(&self, arts: &[Cached], tmp: &Path) -> Result<std::path::PathBuf>;

    /// Place the extracted toolkit into its final destination.
    ///
    /// # Errors
    /// Returns an error if the placement fails.
    fn place(&self, tmp: &Path, dst: &Path, meta: &VersionMeta) -> Result<()>;

    /// Run a post-install smoke test against the placed toolkit.
    ///
    /// # Errors
    /// Returns an error if the smoke test fails.
    fn smoke_test(&self, root: &Path) -> Result<()>;

    /// Ingest a user-supplied artifact file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be ingested.
    fn ingest_supplied(&self, file: &Path, kind: ArtifactKind) -> Result<std::path::PathBuf>;

    /// Scan the system for existing installs that could be adopted.
    ///
    /// # Errors
    /// Returns an error if scanning fails.
    fn scan(&self) -> Result<Vec<Candidate>>;

    /// Adopt a scan candidate into a managed bundle.
    ///
    /// # Errors
    /// Returns an error if adoption fails.
    fn adopt(&self, c: &Candidate) -> Result<Bundle>;
}

pub trait Inventory {
    /// List all managed bundles.
    ///
    /// # Errors
    /// Returns an error if the manifest cannot be loaded.
    fn list(&self) -> Result<Vec<Bundle>>;

    /// Deregister a bundle by handle.
    ///
    /// # Errors
    /// Returns an error if the bundle cannot be deregistered.
    fn deregister(&self, handle: &str) -> Result<()>;

    /// Set an alias to a target spec.
    ///
    /// # Errors
    /// Returns an error if the alias cannot be persisted.
    fn set_alias(&self, n: &str, t: &str) -> Result<()>;

    /// Load the manifest.
    ///
    /// # Errors
    /// Returns an error if the manifest cannot be read or parsed.
    fn load(&self) -> Result<Manifest>;

    /// Save the manifest.
    ///
    /// # Errors
    /// Returns an error if the manifest cannot be written.
    fn save(&self, m: &Manifest) -> Result<()>;
}

pub trait RegistryClient {
    /// List available toolkit versions for a platform.
    ///
    /// # Errors
    /// Returns an error if the registry cannot be queried.
    fn list_toolkits(&self, p: &Platform) -> Result<Vec<Version>>;

    /// List available cuDNN versions for a platform and CUDA major.
    ///
    /// # Errors
    /// Returns an error if the registry cannot be queried.
    fn list_cudnn(&self, p: &Platform, major: u32) -> Result<Vec<Version>>;

    /// Resolve a toolkit version to its component artifacts.
    ///
    /// # Errors
    /// Returns an error if resolution fails.
    fn resolve_toolkit(
        &self,
        v: &Version,
        p: &Platform,
        want: &ComponentPolicy,
    ) -> Result<Vec<Artifact>>;

    /// Resolve a cuDNN version to its artifacts.
    ///
    /// # Errors
    /// Returns an error if resolution fails.
    fn resolve_cudnn(&self, v: &Version, p: &Platform, major: u32) -> Result<Vec<Artifact>>;
}

pub trait DriverProbe {
    /// Probe the installed NVIDIA driver.
    ///
    /// # Errors
    /// Returns an error if the driver cannot be probed.
    fn probe(&self) -> Result<Driver>;
}

pub trait CompatEngine {
    /// The maximum toolkit version supported by the given driver.
    ///
    /// # Errors
    /// Returns an error if the ceiling cannot be determined.
    fn max_toolkit_for_driver(&self, d: &Driver) -> Result<Version>;

    /// Check whether a wanted toolkit is compatible with the driver.
    fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict;

    /// Pick the best-matching cuDNN for a toolkit from the available set.
    fn pair_cudnn(&self, tk: &Version, avail: &[Version]) -> Option<Version>;

    /// Validate a specific toolkit/cuDNN pairing.
    fn validate_pair(&self, tk: &Version, cudnn: &Version) -> Verdict;
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

    // Object-safety witnesses: each must accept a trait object.
    // (If any trait were not object-safe, these fns would fail to compile.)
    fn _assert_resolver_object_safe(_: &dyn Resolver) {}
    fn _assert_activator_object_safe(_: &dyn Activator) {}
    fn _assert_installer_object_safe(_: &dyn Installer) {}
    fn _assert_inventory_object_safe(_: &dyn Inventory) {}
    fn _assert_registry_object_safe(_: &dyn RegistryClient) {}
    fn _assert_driverprobe_object_safe(_: &dyn DriverProbe) {}
    fn _assert_compat_object_safe(_: &dyn CompatEngine) {}

    #[test]
    fn ports_are_object_safe() {
        // Compiling the witnesses above is the assertion; this test just anchors them.
        fn takes_fn(_f: fn(&dyn Resolver)) {}
        takes_fn(_assert_resolver_object_safe);
    }
}
