//! cuvm-registry — parse `redistrib_<ver>.json` (serde flatten, dynamic component
//! keys) and resolve toolkit components into `cuvm_app::Artifact`s.
//!
//! All HTTP is delegated to `cuvm_download::http_get` (registry depends on download
//! per the workspace dependency rule). This crate never constructs redist file
//! names: it copies `relative_path` verbatim and joins it onto the base URL.

#![forbid(unsafe_code)]

use thiserror::Error;

/// Errors raised while querying or parsing the CUDA redist registry.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// A `redistrib_<ver>.json` body did not parse as a redist manifest.
    #[error("failed to parse redist manifest: {0}")]
    Parse(String),

    /// The redist index HTML contained no `redistrib_<ver>.json` links.
    #[error("no redistrib_<ver>.json links found in redist index at {url}")]
    EmptyIndex {
        /// The index URL that was scraped.
        url: String,
    },

    /// The manifest had no object for the requested redist platform key.
    #[error("component `{component}` has no `{platform}` artifact in this manifest")]
    MissingPlatform {
        /// The component whose platform object was missing.
        component: String,
        /// The redist platform key that was requested (e.g. `linux-x86_64`).
        platform: String,
    },

    /// None of the recommended/requested components were present in the manifest.
    #[error("no usable components found for this toolkit (wanted: {wanted})")]
    NoComponents {
        /// A human-readable join of the requested component names.
        wanted: String,
    },

    /// An underlying HTTP fetch (via `cuvm_download::http_get`) failed.
    #[error("registry HTTP request failed: {0}")]
    Http(String),
}

/// `Result` alias for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;

use std::collections::BTreeMap;

use serde::Deserialize;

/// One redist platform object — mirrors a single `{relative_path, sha256, md5?, size}`
/// entry under a component. `md5` is optional (some components omit it).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RedistArtifact {
    /// Path under the redist base URL, copied verbatim (never reconstructed).
    pub relative_path: String,
    /// Hex SHA-256 from the manifest; always verified before use.
    pub sha256: String,
    /// Optional hex MD5 from the manifest.
    #[serde(default)]
    pub md5: Option<String>,
    /// Compressed artifact size in bytes.
    pub size: u64,
}

/// One redist component (e.g. `cuda_nvcc`). Per-platform objects are flattened into
/// `platforms`, keyed by redist platform key (`linux-x86_64`, `windows-x86_64`, …).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RedistComponent {
    /// Human-readable component name, if present.
    #[serde(default)]
    pub name: Option<String>,
    /// Component version string (independent within a release), if present.
    #[serde(default)]
    pub version: Option<String>,
    /// License label, if present.
    #[serde(default)]
    pub license: Option<String>,
    /// Per-platform artifacts; any unknown string scalars (`name`/`version`/…) are
    /// captured above, so `flatten` here only collects the platform objects.
    #[serde(flatten)]
    pub platforms: BTreeMap<String, RedistArtifact>,
}

/// A parsed `redistrib_<ver>.json`. Only object-valued top-level keys become
/// components; metadata string keys (`release_date`, `release_label`, …) are dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedistManifest {
    /// Components keyed by their redist name, ordered for deterministic iteration.
    pub components: BTreeMap<String, RedistComponent>,
}

/// Each top-level value is either a component object or a metadata scalar string.
/// `untagged` makes serde try `RedistComponent` first, then fall back to a string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TopLevel {
    Component(RedistComponent),
    /// A metadata scalar (`release_date`, `release_label`, …); the captured string
    /// is intentionally discarded — only its existence as a non-object matters.
    Meta(#[allow(dead_code)] String),
}

impl RedistManifest {
    /// Parse a `redistrib_<ver>.json` body, keeping only object component keys.
    ///
    /// # Errors
    /// Returns [`RegistryError::Parse`] if `json` is not a valid redist manifest.
    pub fn parse(json: &str) -> RegistryResult<Self> {
        let raw: BTreeMap<String, TopLevel> =
            serde_json::from_str(json).map_err(|e| RegistryError::Parse(e.to_string()))?;
        let mut components = BTreeMap::new();
        for (key, value) in raw {
            if let TopLevel::Component(c) = value {
                components.insert(key, c);
            }
        }
        Ok(RedistManifest { components })
    }

    /// Look up a component by its redist name (e.g. `cuda_nvcc`).
    #[must_use]
    pub fn component(&self, name: &str) -> Option<&RedistComponent> {
        self.components.get(name)
    }
}

/// The minimal usable component set for a CUDA `major`, filtered to what is actually
/// present in the parsed manifest (so missing components are dropped, never invented).
///
/// - 12.x ⇒ `cuda_nvcc`, `cuda_cudart`, `cuda_nvrtc`.
/// - 13.x ⇒ `cuda_nvcc`, `cuda_cudart`, `cuda_crt`, the CCCL component (`cccl` at
///   13.3+, else `cuda_cccl`), `libnvvm`, `cuda_nvrtc`.
///
/// The CCCL key is resolved dynamically from `present`, handling the 13.3 rename.
#[must_use]
pub fn recommended_components(
    cuda_major: u32,
    present: &BTreeMap<String, RedistComponent>,
) -> Vec<String> {
    let wanted: Vec<&str> = if cuda_major >= 13 {
        // Prefer the new `cccl` spelling, fall back to the pre-13.3 `cuda_cccl`.
        let cccl = if present.contains_key("cccl") {
            "cccl"
        } else {
            "cuda_cccl"
        };
        vec![
            "cuda_nvcc",
            "cuda_cudart",
            "cuda_crt",
            cccl,
            "libnvvm",
            "cuda_nvrtc",
        ]
    } else {
        vec!["cuda_nvcc", "cuda_cudart", "cuda_nvrtc"]
    };

    wanted
        .into_iter()
        .filter(|name| present.contains_key(*name))
        .map(String::from)
        .collect()
}

use cuvm_core::{Platform, Version};

/// The production CUDA redist base URL (trailing slash required).
const DEFAULT_BASE_URL: &str = "https://developer.download.nvidia.com/compute/cuda/redist/";

/// Default `cuvm_app::RegistryClient`: scrapes the redist index and resolves
/// `redistrib_<ver>.json` manifests into `Artifact`s. All HTTP goes through
/// `cuvm_download::http_get`.
#[derive(Debug, Clone)]
pub struct DefaultRegistryClient {
    base_url: String,
}

impl Default for DefaultRegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultRegistryClient {
    /// Build a client pointed at the production redist base URL.
    #[must_use]
    pub fn new() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Build a client pointed at a custom base URL (a trailing `/` is enforced).
    /// Tests point this at an `httpmock` server.
    #[must_use]
    pub fn with_base_url(url: String) -> Self {
        let base_url = if url.ends_with('/') {
            url
        } else {
            format!("{url}/")
        };
        Self { base_url }
    }

    /// Fetch a URL as UTF-8 text via `cuvm_download::http_get`.
    fn get_text(url: &str) -> RegistryResult<String> {
        let bytes = cuvm_download::http_get(url).map_err(|e| RegistryError::Http(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| RegistryError::Http(e.to_string()))
    }
}

/// Extract every distinct `X.Y.Z` from `redistrib_<X.Y.Z>.json` substrings in `html`.
/// Pure string walk — no regex/HTML dependency.
fn scrape_redistrib_versions(html: &str) -> Vec<String> {
    const PREFIX: &str = "redistrib_";
    const SUFFIX: &str = ".json";
    let mut out: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(PREFIX) {
        let after = &rest[start + PREFIX.len()..];
        if let Some(end) = after.find(SUFFIX) {
            let ver = &after[..end];
            // Accept only dotted-numeric version bodies (guards against false hits).
            if !ver.is_empty()
                && ver.chars().all(|c| c.is_ascii_digit() || c == '.')
                && ver.contains('.')
            {
                let owned = ver.to_string();
                if !out.contains(&owned) {
                    out.push(owned);
                }
            }
            rest = &after[end + SUFFIX.len()..];
        } else {
            rest = after;
        }
    }
    out
}

impl cuvm_app::RegistryClient for DefaultRegistryClient {
    fn list_toolkits(&self, _p: &Platform) -> anyhow::Result<Vec<Version>> {
        // NVIDIA serves the redist directory listing at the base URL itself.
        let index_url = self.base_url.clone();
        let html = Self::get_text(&index_url)?;
        let mut versions: Vec<Version> = scrape_redistrib_versions(&html)
            .into_iter()
            .filter_map(|s| Version::parse(&s).ok())
            .collect();
        if versions.is_empty() {
            return Err(RegistryError::EmptyIndex { url: index_url }.into());
        }
        versions.sort();
        versions.dedup();
        Ok(versions)
    }

    fn list_cudnn(&self, _p: &Platform, _major: u32) -> anyhow::Result<Vec<Version>> {
        // cuDNN registry listing lands in M3; M2 install path does not call this.
        anyhow::bail!("list_cudnn is not implemented in M2")
    }

    fn resolve_toolkit(
        &self,
        v: &Version,
        p: &Platform,
        want: &cuvm_app::ComponentPolicy,
    ) -> anyhow::Result<Vec<cuvm_app::Artifact>> {
        // `Platform` is small + `Copy`; pass by value into the helper so clippy's
        // `trivially_copy_pass_by_ref` is satisfied on the private signature.
        self.resolve_toolkit_impl(v, *p, want).map_err(Into::into)
    }

    fn resolve_cudnn(
        &self,
        _v: &Version,
        _p: &Platform,
        _major: u32,
    ) -> anyhow::Result<Vec<cuvm_app::Artifact>> {
        anyhow::bail!("resolve_cudnn is not implemented in M2")
    }
}

impl DefaultRegistryClient {
    /// Fetch `redistrib_<v>.json`, choose the component set, and emit one
    /// [`cuvm_app::Artifact`] per `(component, platform)` with `url` = base + path.
    fn resolve_toolkit_impl(
        &self,
        v: &Version,
        p: Platform,
        want: &cuvm_app::ComponentPolicy,
    ) -> RegistryResult<Vec<cuvm_app::Artifact>> {
        let manifest_url = format!("{}redistrib_{}.json", self.base_url, v.raw);
        let body = Self::get_text(&manifest_url)?;
        let manifest = RedistManifest::parse(&body)?;

        let names: Vec<String> = match want {
            cuvm_app::ComponentPolicy::Recommended => {
                recommended_components(v.major(), &manifest.components)
            }
            cuvm_app::ComponentPolicy::Only(list) => list
                .iter()
                .filter(|n| manifest.components.contains_key(n.as_str()))
                .cloned()
                .collect(),
        };

        if names.is_empty() {
            let wanted = match want {
                cuvm_app::ComponentPolicy::Recommended => "recommended set".to_string(),
                cuvm_app::ComponentPolicy::Only(list) => list.join(", "),
            };
            return Err(RegistryError::NoComponents { wanted });
        }

        let platform_key = p.redist_key();
        let mut artifacts = Vec::with_capacity(names.len());
        for name in names {
            // `name` was filtered to present keys above, so this lookup succeeds.
            let component =
                manifest
                    .component(&name)
                    .ok_or_else(|| RegistryError::NoComponents {
                        wanted: name.clone(),
                    })?;
            let redist = component.platforms.get(&platform_key).ok_or_else(|| {
                RegistryError::MissingPlatform {
                    component: name.clone(),
                    platform: platform_key.clone(),
                }
            })?;
            artifacts.push(cuvm_app::Artifact {
                component: name,
                relative_path: redist.relative_path.clone(),
                url: format!("{}{}", self.base_url, redist.relative_path),
                sha256: redist.sha256.clone(),
                md5: redist.md5.clone(),
                size: redist.size,
            });
        }
        Ok(artifacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_wraps_serde_message() {
        let err = RegistryError::Parse("expected value at line 1".to_string());
        assert_eq!(
            err.to_string(),
            "failed to parse redist manifest: expected value at line 1"
        );
    }

    #[test]
    fn empty_index_error_has_stable_message() {
        let err = RegistryError::EmptyIndex {
            url: "https://example.invalid/redist/".to_string(),
        };
        assert!(err.to_string().contains("no redistrib_<ver>.json links"));
        assert!(err.to_string().contains("https://example.invalid/redist/"));
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    // A 13.3-style fixture: metadata string keys interleaved with object component
    // keys, the CCCL component spelled `cccl` (renamed from `cuda_cccl` at 13.3),
    // and both linux-x86_64 + windows-x86_64 platform objects.
    const REDIST_133: &str = r#"{
      "release_date": "2025-03-01",
      "release_label": "13.3.0",
      "cuda_nvcc": {
        "name": "CUDA NVCC",
        "version": "13.3.33",
        "license": "CUDA Toolkit",
        "linux-x86_64": {
          "relative_path": "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-13.3.33-archive.tar.xz",
          "sha256": "aaa111",
          "md5": "m1",
          "size": 100
        },
        "windows-x86_64": {
          "relative_path": "cuda_nvcc/windows-x86_64/cuda_nvcc-windows-x86_64-13.3.33-archive.zip",
          "sha256": "aaa222",
          "size": 200
        }
      },
      "cccl": {
        "name": "CXX Core Compute Libraries",
        "version": "13.3.3.3.1",
        "linux-x86_64": {
          "relative_path": "cccl/linux-x86_64/cccl-linux-x86_64-13.3.3.3.1-archive.tar.xz",
          "sha256": "ccc111",
          "size": 50
        }
      }
    }"#;

    // A 12.x fixture: only cuda_nvcc + cuda_cudart, no md5 on cudart.
    const REDIST_124: &str = r#"{
      "release_label": "12.4.1",
      "cuda_nvcc": {
        "version": "12.4.131",
        "linux-x86_64": {
          "relative_path": "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-12.4.131-archive.tar.xz",
          "sha256": "deadbeef",
          "md5": "abc",
          "size": 1234
        }
      },
      "cuda_cudart": {
        "version": "12.4.127",
        "linux-x86_64": {
          "relative_path": "cuda_cudart/linux-x86_64/cuda_cudart-linux-x86_64-12.4.127-archive.tar.xz",
          "sha256": "feedface",
          "size": 5678
        }
      }
    }"#;

    #[test]
    fn parse_keeps_only_object_component_keys() {
        let m = RedistManifest::parse(REDIST_133).expect("parse 13.3");
        // metadata string keys must NOT become components.
        assert!(m.component("release_date").is_none());
        assert!(m.component("release_label").is_none());
        // object keys must.
        assert!(m.component("cuda_nvcc").is_some());
        assert!(
            m.component("cccl").is_some(),
            "13.3 uses `cccl` not `cuda_cccl`"
        );
        // exactly two real components.
        assert_eq!(m.components.len(), 2);
    }

    #[test]
    fn component_exposes_per_platform_artifacts_with_verbatim_paths() {
        let m = RedistManifest::parse(REDIST_133).unwrap();
        let nvcc = m.component("cuda_nvcc").unwrap();
        assert_eq!(nvcc.name.as_deref(), Some("CUDA NVCC"));
        assert_eq!(nvcc.version.as_deref(), Some("13.3.33"));
        let lin = nvcc.platforms.get("linux-x86_64").expect("linux object");
        assert_eq!(
            lin.relative_path,
            "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-13.3.33-archive.tar.xz"
        );
        assert_eq!(lin.sha256, "aaa111");
        assert_eq!(lin.md5.as_deref(), Some("m1"));
        assert_eq!(lin.size, 100);
        // windows object present, md5 absent (Option) → None, not an error.
        let win = nvcc.platforms.get("windows-x86_64").unwrap();
        assert_eq!(win.md5, None);
        assert_eq!(win.size, 200);
    }

    #[test]
    fn parse_12x_minimal_set() {
        let m = RedistManifest::parse(REDIST_124).unwrap();
        assert_eq!(m.components.len(), 2);
        let cudart = m.component("cuda_cudart").unwrap();
        let lin = cudart.platforms.get("linux-x86_64").unwrap();
        assert_eq!(lin.sha256, "feedface");
        assert_eq!(lin.md5, None);
    }

    #[test]
    fn parse_rejects_non_json() {
        let err = RedistManifest::parse("<html>not json</html>").unwrap_err();
        assert!(matches!(err, RegistryError::Parse(_)));
    }
}

#[cfg(test)]
mod recommended_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn present(keys: &[&str]) -> BTreeMap<String, RedistComponent> {
        keys.iter()
            .map(|k| {
                (
                    (*k).to_string(),
                    RedistComponent {
                        name: None,
                        version: None,
                        license: None,
                        platforms: BTreeMap::new(),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn twelve_x_is_nvcc_cudart_nvrtc() {
        let p = present(&["cuda_nvcc", "cuda_cudart", "cuda_nvrtc", "libcublas"]);
        let got = recommended_components(12, &p);
        assert_eq!(got, vec!["cuda_nvcc", "cuda_cudart", "cuda_nvrtc"]);
    }

    #[test]
    fn thirteen_three_uses_cccl_rename() {
        // 13.3 spells CCCL as `cccl`.
        let p = present(&[
            "cuda_nvcc",
            "cuda_cudart",
            "cuda_crt",
            "cccl",
            "libnvvm",
            "cuda_nvrtc",
        ]);
        let got = recommended_components(13, &p);
        assert_eq!(
            got,
            vec![
                "cuda_nvcc",
                "cuda_cudart",
                "cuda_crt",
                "cccl",
                "libnvvm",
                "cuda_nvrtc"
            ]
        );
    }

    #[test]
    fn thirteen_zero_uses_cuda_cccl_rename() {
        // 13.0–13.2 spell CCCL as `cuda_cccl`; the resolver must pick the present key.
        let p = present(&[
            "cuda_nvcc",
            "cuda_cudart",
            "cuda_crt",
            "cuda_cccl",
            "libnvvm",
            "cuda_nvrtc",
        ]);
        let got = recommended_components(13, &p);
        assert_eq!(
            got,
            vec![
                "cuda_nvcc",
                "cuda_cudart",
                "cuda_crt",
                "cuda_cccl",
                "libnvvm",
                "cuda_nvrtc"
            ]
        );
    }

    #[test]
    fn missing_components_are_skipped() {
        // manifest lacks cuda_nvrtc → it is dropped, not invented.
        let p = present(&["cuda_nvcc", "cuda_cudart"]);
        let got = recommended_components(12, &p);
        assert_eq!(got, vec!["cuda_nvcc", "cuda_cudart"]);
    }
}
