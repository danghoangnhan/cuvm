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

    /// The platform exists in the cuDNN manifest but has no build for the
    /// requested CUDA major (e.g. cuDNN 8.9.7 ships no `cuda13` variant).
    #[error("platform `{platform}` has no `{variant}` build in this cuDNN manifest")]
    MissingCudaVariant {
        /// The redist platform key that was looked up (e.g. `windows-x86_64`).
        platform: String,
        /// The `cuda<major>` variant key that was absent (e.g. `cuda13`).
        variant: String,
    },

    /// None of the recommended/requested components were present in the manifest.
    #[error("no usable components found for this toolkit (wanted: {wanted})")]
    NoComponents {
        /// A human-readable join of the requested component names.
        wanted: String,
    },

    /// The NCCL `vX.Y.Z/` directory listed no archive matching the requested
    /// architecture + CUDA major (e.g. no `cuda13` NCCL build, or a non-Linux
    /// platform the mirror does not serve).
    #[error("no NCCL build for {arch} + cuda{cuda_major} in nccl/v{version}/")]
    NoNcclBuild {
        /// The NCCL version directory that was listed (e.g. `2.21.5`).
        version: String,
        /// The NCCL architecture token that was requested (e.g. `x86_64`).
        arch: String,
        /// The CUDA major the build was requested for.
        cuda_major: u32,
    },

    /// An underlying HTTP fetch (via `cuvm_download::http_get`) failed.
    #[error("registry HTTP request failed: {0}")]
    Http(String),
}

/// `Result` alias for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;

use std::collections::BTreeMap;

use serde::Deserialize;

/// NVIDIA manifests have flipped `size` between JSON number and string across
/// products and eras (see commit 0774819); both `de_size` and `de_size_opt`
/// accept either representation via this untagged shape.
#[derive(Deserialize)]
#[serde(untagged)]
enum SizeRepr {
    Num(u64),
    Text(String),
}

impl SizeRepr {
    /// Collapse to bytes; an unparseable string is a real error, never 0.
    fn into_u64<E: serde::de::Error>(self) -> Result<u64, E> {
        match self {
            SizeRepr::Num(n) => Ok(n),
            SizeRepr::Text(s) => s.trim().parse().map_err(serde::de::Error::custom),
        }
    }
}

/// NVIDIA redist manifests serialize `size` as a JSON string in production
/// (`"size": "1099680"`); accept both string and number.
fn de_size<'de, D>(de: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    SizeRepr::deserialize(de)?.into_u64()
}

/// Optional variant of [`de_size`]: absent/`null` stays `None`; present values
/// accept both string and number. A present-but-unparseable string is a real
/// error (matching `de_size`'s posture), not a silent `None`.
fn de_size_opt<'de, D>(de: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<SizeRepr>::deserialize(de)?
        .map(SizeRepr::into_u64)
        .transpose()
}

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
    #[serde(deserialize_with = "de_size")]
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

/// One artifact under a cuDNN platform's `cuda<major>` variant key.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CudnnVariantArtifact {
    /// Path under the cuDNN redist base URL, copied verbatim.
    pub relative_path: String,
    /// Hex SHA-256 from the manifest; always verified before use.
    pub sha256: String,
    /// Optional hex MD5 from the manifest.
    #[serde(default)]
    pub md5: Option<String>,
    /// Payload size in bytes; tolerant of the string/number representation
    /// flip (same hazard class as `de_size` — see commit 0774819).
    #[serde(default, deserialize_with = "de_size_opt")]
    pub size: Option<u64>,
}

impl CudnnVariantArtifact {
    /// Size in bytes, 0 when the manifest omits it (only progress totals
    /// consume it, and those come from Content-Length anyway).
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        self.size.unwrap_or(0)
    }
}

/// The `cudnn` product out of a per-product cuDNN redist manifest
/// (`redistrib_<label>.json` at the cuDNN base). Platform values nest one
/// level deeper than toolkit manifests: platform key -> `cuda<major>` -> artifact.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CudnnManifest {
    /// The product's own 4-field version (e.g. `9.23.0.39`); the index label
    /// (`9.23.0`) is what cuvm uses as the user-facing handle.
    pub product_version: Option<String>,
    platforms: BTreeMap<String, BTreeMap<String, CudnnVariantArtifact>>,
}

impl CudnnManifest {
    /// Parse a cuDNN redist manifest, keeping only the `cudnn` product.
    ///
    /// # Errors
    /// [`RegistryError::Parse`] when the document is not JSON or has no
    /// `cudnn` product object.
    pub fn parse(json: &str) -> RegistryResult<Self> {
        #[derive(Deserialize)]
        struct Doc {
            cudnn: Option<RawProduct>,
        }
        #[derive(Deserialize)]
        struct RawProduct {
            #[serde(default)]
            version: Option<String>,
            #[serde(flatten)]
            rest: BTreeMap<String, serde_json::Value>,
        }

        let doc: Doc =
            serde_json::from_str(json).map_err(|e| RegistryError::Parse(e.to_string()))?;
        let product = doc
            .cudnn
            .ok_or_else(|| RegistryError::Parse("manifest has no `cudnn` product".into()))?;

        // Platform keys are the entries whose value is a map of
        // `cuda<major>` -> artifact; metadata keys (name/license/cuda_variant)
        // fail that shape and are skipped.
        let platforms = product
            .rest
            .into_iter()
            .filter_map(|(key, value)| {
                let variants: BTreeMap<String, CudnnVariantArtifact> =
                    serde_json::from_value(value).ok()?;
                (!variants.is_empty() && variants.keys().all(|k| k.starts_with("cuda")))
                    .then_some((key, variants))
            })
            .collect();

        Ok(CudnnManifest {
            product_version: product.version,
            platforms,
        })
    }

    /// The artifact for `platform_key` (e.g. `linux-x86_64`) and CUDA major.
    ///
    /// # Errors
    /// [`RegistryError::MissingPlatform`] when the platform has no builds at
    /// all; [`RegistryError::MissingCudaVariant`] when it exists but lacks the
    /// `cuda<major>` variant.
    pub fn artifact(
        &self,
        platform_key: &str,
        cuda_major: u32,
    ) -> RegistryResult<&CudnnVariantArtifact> {
        let variants =
            self.platforms
                .get(platform_key)
                .ok_or_else(|| RegistryError::MissingPlatform {
                    component: "cudnn".into(),
                    platform: platform_key.into(),
                })?;
        let variant = format!("cuda{cuda_major}");
        variants
            .get(&variant)
            .ok_or_else(|| RegistryError::MissingCudaVariant {
                platform: platform_key.into(),
                variant,
            })
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

/// NVIDIA's account-free cuDNN redistributables index (spec §2.3).
const DEFAULT_CUDNN_BASE_URL: &str = "https://developer.download.nvidia.com/compute/cudnn/redist/";

/// NVIDIA's NCCL redistributables index (spec §2.3): a plain `vX.Y.Z/`
/// directory listing with NO JSON manifest and NO checksums — cuvm
/// self-records each archive's sha256.
const DEFAULT_NCCL_BASE_URL: &str = "https://developer.download.nvidia.com/compute/redist/nccl/";

/// Default `cuvm_app::RegistryClient`: scrapes the redist index and resolves
/// `redistrib_<ver>.json` manifests into `Artifact`s. All HTTP goes through
/// `cuvm_download::http_get`.
#[derive(Debug, Clone)]
// The shared `_base_url` suffix is intentional: one field per product redist,
// each with a matching `*_base_url()` accessor. Renaming to drop the suffix
// would only desync the fields from their getters.
#[allow(clippy::struct_field_names)]
pub struct DefaultRegistryClient {
    base_url: String,
    cudnn_base_url: String,
    nccl_base_url: String,
}

impl Default for DefaultRegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Enforce the trailing `/` every base URL needs: artifact URLs are formed as
/// `base + relative_path`.
fn with_trailing_slash(url: String) -> String {
    if url.ends_with('/') {
        url
    } else {
        format!("{url}/")
    }
}

impl DefaultRegistryClient {
    /// Production NVIDIA endpoints for every product.
    #[must_use]
    pub fn new() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            cudnn_base_url: DEFAULT_CUDNN_BASE_URL.to_string(),
            nccl_base_url: DEFAULT_NCCL_BASE_URL.to_string(),
        }
    }

    /// Override the CUDA toolkit base only (cuDNN/NCCL keep their production
    /// defaults). Trailing slash enforced: artifact URLs are `base + relative_path`.
    #[must_use]
    pub fn with_base_url(url: String) -> Self {
        Self {
            base_url: with_trailing_slash(url),
            ..Self::new()
        }
    }

    /// Override every product base (tests point each at a mock).
    #[must_use]
    pub fn with_base_urls(cuda: String, cudnn: String, nccl: String) -> Self {
        Self {
            base_url: with_trailing_slash(cuda),
            cudnn_base_url: with_trailing_slash(cudnn),
            nccl_base_url: with_trailing_slash(nccl),
        }
    }

    /// The CUDA toolkit redist base URL (always trailing-slash terminated).
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The cuDNN redist base URL (always trailing-slash terminated).
    #[must_use]
    pub fn cudnn_base_url(&self) -> &str {
        &self.cudnn_base_url
    }

    /// The NCCL redist base URL (always trailing-slash terminated).
    #[must_use]
    pub fn nccl_base_url(&self) -> &str {
        &self.nccl_base_url
    }

    /// Fetch a URL as UTF-8 text via `cuvm_download::http_get`.
    fn get_text(url: &str) -> RegistryResult<String> {
        let bytes = cuvm_download::http_get(url).map_err(|e| RegistryError::Http(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| RegistryError::Http(e.to_string()))
    }

    /// Scrape `redistrib_<ver>.json` links at `index_url` into sorted, deduped
    /// versions. Shared body of `list_toolkits`/`list_cudnn` — the two
    /// products differ only in which index URL they scrape.
    fn list_versions_at(index_url: &str) -> RegistryResult<Vec<Version>> {
        let html = Self::get_text(index_url)?;
        let mut versions: Vec<Version> = scrape_redistrib_versions(&html)
            .into_iter()
            .filter_map(|s| Version::parse(&s).ok())
            .collect();
        if versions.is_empty() {
            return Err(RegistryError::EmptyIndex {
                url: index_url.to_string(),
            });
        }
        versions.sort();
        versions.dedup();
        Ok(versions)
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

/// Extract `MAJOR.MINOR[.PATCH]` from `vMAJOR.MINOR.../` directory links in the
/// NCCL index HTML (e.g. `href='v2.21.5/'` → `2.21.5`). Pure string walk; the
/// trailing `/` requirement rejects bare `v2` mentions in prose, and junk dirs
/// like `New folder/` never start with `v<digit>`.
fn scrape_nccl_versions(html: &str) -> Vec<String> {
    let bytes = html.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // A version dir starts with `v` followed by a digit.
        if bytes[i] == b'v' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            // Only a `<dotted-numeric>/` token is a directory link.
            if bytes.get(j) == Some(&b'/') {
                let ver = &html[start..j];
                if ver.contains('.') && !ver.ends_with('.') && !out.contains(&ver.to_string()) {
                    out.push(ver.to_string());
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// One parsed NCCL archive file name:
/// `nccl_<ncclver>-<build>+cuda<MAJOR.MINOR>_<arch>.txz`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NcclArchive {
    /// NCCL version (the `<ncclver>` field, e.g. `2.21.5`).
    nccl_version: String,
    cuda_major: u32,
    cuda_minor: u32,
    /// NCCL arch token (`x86_64` / `aarch64` / `ppc64le`).
    arch: String,
    /// The verbatim file name (joined onto `vX.Y.Z/` for the URL).
    file_name: String,
}

/// Parse an `nccl_<ver>-<build>+cuda<X.Y>_<arch>.txz` file name. Returns `None`
/// for anything that does not match the redist convention.
fn parse_nccl_filename(name: &str) -> Option<NcclArchive> {
    let stem = name.strip_prefix("nccl_")?.strip_suffix(".txz")?;
    // stem = "2.21.5-1+cuda12.4_x86_64"
    let (ver_build, cuda_arch) = stem.split_once("+cuda")?;
    // The NCCL version is the field before the first `-` (the build suffix).
    let nccl_version = ver_build.split_once('-').map_or(ver_build, |(v, _)| v);
    if nccl_version.is_empty() || !nccl_version.contains('.') {
        return None;
    }
    // cuda_arch = "12.4_x86_64" → ("12.4", "x86_64"); arch may itself contain `_`.
    let (cuda, arch) = cuda_arch.split_once('_')?;
    let (maj, min) = cuda.split_once('.')?;
    Some(NcclArchive {
        nccl_version: nccl_version.to_string(),
        cuda_major: maj.parse().ok()?,
        cuda_minor: min.parse().ok()?,
        arch: arch.to_string(),
        file_name: name.to_string(),
    })
}

/// Map a cuvm `Platform` to the NCCL mirror's arch token. The NCCL redist
/// serves Linux only; `None` means cuvm has no NCCL build for this platform.
fn nccl_arch(p: Platform) -> Option<&'static str> {
    use cuvm_core::{Arch, Os};
    match (p.os, p.arch) {
        (Os::Linux, Arch::X86_64) => Some("x86_64"),
        (Os::Linux, Arch::Aarch64 | Arch::Sbsa) => Some("aarch64"),
        (Os::Windows, _) => None,
    }
}

/// Extract every `nccl_<...>.txz` file name from an NCCL version-directory
/// listing (the name appears twice per file — href + text — so dedupe).
fn scrape_nccl_files(html: &str) -> Vec<String> {
    const PREFIX: &str = "nccl_";
    const SUFFIX: &str = ".txz";
    let mut out: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(PREFIX) {
        let after = &rest[start..];
        if let Some(end) = after.find(SUFFIX) {
            let file = &after[..end + SUFFIX.len()];
            // A real file name carries no markup/quote/space/slash characters.
            if !file.contains(['\'', '"', ' ', '/', '<', '>']) && !out.contains(&file.to_string()) {
                out.push(file.to_string());
            }
            rest = &after[end + SUFFIX.len()..];
        } else {
            break;
        }
    }
    out
}

impl cuvm_app::RegistryClient for DefaultRegistryClient {
    fn list_toolkits(&self, _p: &Platform) -> anyhow::Result<Vec<Version>> {
        // NVIDIA serves the redist directory listing at the base URL itself.
        Self::list_versions_at(&self.base_url).map_err(Into::into)
    }

    /// Versions named by the cuDNN redist index, ascending. Like
    /// `list_toolkits`, this is index-only: the index is platform-agnostic, so
    /// `_p`/`_major` are accepted for the port shape but not used as filters —
    /// platform/variant gaps surface at `resolve_cudnn` (D4).
    fn list_cudnn(&self, _p: &Platform, _major: u32) -> anyhow::Result<Vec<Version>> {
        Self::list_versions_at(&self.cudnn_base_url).map_err(Into::into)
    }

    /// NCCL versions named by the `nccl/` directory index, ascending. Index-only
    /// (the listing is platform-agnostic); platform/CUDA gaps surface at
    /// `resolve_nccl`.
    fn list_nccl(&self, _p: &Platform) -> anyhow::Result<Vec<Version>> {
        let html = Self::get_text(&self.nccl_base_url)?;
        let mut versions: Vec<Version> = scrape_nccl_versions(&html)
            .into_iter()
            .filter_map(|s| Version::parse(&s).ok())
            .collect();
        if versions.is_empty() {
            return Err(RegistryError::EmptyIndex {
                url: self.nccl_base_url.clone(),
            }
            .into());
        }
        versions.sort();
        versions.dedup();
        Ok(versions)
    }

    /// The single NCCL artifact for this version/platform/CUDA-major, picking
    /// the newest `cuda<major>.*` build. The artifact's `sha256` is empty — the
    /// NCCL redist publishes no checksums, so the caller self-records it.
    fn resolve_nccl(
        &self,
        v: &Version,
        p: &Platform,
        cuda_major: u32,
    ) -> anyhow::Result<Vec<cuvm_app::Artifact>> {
        self.resolve_nccl_impl(v, *p, cuda_major)
            .map_err(Into::into)
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

    /// The single `cudnn` artifact for this version/platform/CUDA-major.
    fn resolve_cudnn(
        &self,
        v: &Version,
        p: &Platform,
        major: u32,
    ) -> anyhow::Result<Vec<cuvm_app::Artifact>> {
        let manifest_url = format!("{}redistrib_{}.json", self.cudnn_base_url, v.raw);
        let manifest = CudnnManifest::parse(&Self::get_text(&manifest_url)?)?;
        let art = manifest.artifact(&p.redist_key(), major)?;
        Ok(vec![cuvm_app::Artifact {
            component: "cudnn".to_string(),
            relative_path: art.relative_path.clone(),
            url: format!("{}{}", self.cudnn_base_url, art.relative_path),
            sha256: art.sha256.clone(),
            md5: art.md5.clone(),
            size: art.size_bytes(),
        }])
    }
}

impl DefaultRegistryClient {
    /// List `nccl/v<v>/`, pick the newest `cuda<major>.*` build for `p`'s arch,
    /// and emit one [`cuvm_app::Artifact`] with an **empty `sha256`** (the NCCL
    /// redist ships no checksums — the caller self-records the hash).
    fn resolve_nccl_impl(
        &self,
        v: &Version,
        p: Platform,
        cuda_major: u32,
    ) -> RegistryResult<Vec<cuvm_app::Artifact>> {
        let arch = nccl_arch(p).ok_or_else(|| RegistryError::NoNcclBuild {
            version: v.raw.clone(),
            arch: p.redist_key(),
            cuda_major,
        })?;
        let dir_url = format!("{}v{}/", self.nccl_base_url, v.raw);
        let html = Self::get_text(&dir_url)?;
        let art = scrape_nccl_files(&html)
            .iter()
            .filter_map(|f| parse_nccl_filename(f))
            .filter(|a| a.arch == arch && a.cuda_major == cuda_major)
            .max_by_key(|a| a.cuda_minor)
            .ok_or_else(|| RegistryError::NoNcclBuild {
                version: v.raw.clone(),
                arch: arch.to_string(),
                cuda_major,
            })?;
        let relative_path = format!("v{}/{}", v.raw, art.file_name);
        Ok(vec![cuvm_app::Artifact {
            component: "nccl".to_string(),
            url: format!("{}{}", self.nccl_base_url, relative_path),
            relative_path,
            sha256: String::new(),
            md5: None,
            size: 0,
        }])
    }

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

    #[test]
    fn base_urls_are_normalized_with_trailing_slashes() {
        let c = DefaultRegistryClient::with_base_urls(
            "http://cuda.example/redist".into(),
            "http://cudnn.example/redist".into(),
            "http://nccl.example/redist".into(),
        );
        assert_eq!(c.base_url(), "http://cuda.example/redist/");
        assert_eq!(c.cudnn_base_url(), "http://cudnn.example/redist/");
        assert_eq!(c.nccl_base_url(), "http://nccl.example/redist/");
        // with_base_url keeps the production cuDNN + NCCL defaults.
        let d = DefaultRegistryClient::with_base_url("http://cuda.example/".into());
        assert_eq!(
            d.cudnn_base_url(),
            "https://developer.download.nvidia.com/compute/cudnn/redist/"
        );
        assert_eq!(
            d.nccl_base_url(),
            "https://developer.download.nvidia.com/compute/redist/nccl/"
        );
    }
}

#[cfg(test)]
mod nccl_tests {
    use super::*;

    /// A trimmed copy of the live `nccl/` index (version dirs + junk entries).
    const NCCL_INDEX: &str = r"<html><body>
        <a href='..'>..</a>
        <a href='New folder/'>New folder/</a>
        <a href='v2.20.5/'>v2.20.5/</a>
        <a href='v2.21.5/'>v2.21.5/</a>
        <a href='v2.30.7/'>v2.30.7/</a>
    </body></html>";

    /// A trimmed copy of a live `nccl/v2.21.5/` directory listing.
    const NCCL_DIR_2215: &str = r"<html><body>
        <a href='..'>..</a>
        <a href='nccl_2.21.5-1+cuda11.0_x86_64.txz'>nccl_2.21.5-1+cuda11.0_x86_64.txz</a>
        <a href='nccl_2.21.5-1+cuda12.2_x86_64.txz'>nccl_2.21.5-1+cuda12.2_x86_64.txz</a>
        <a href='nccl_2.21.5-1+cuda12.4_x86_64.txz'>nccl_2.21.5-1+cuda12.4_x86_64.txz</a>
        <a href='nccl_2.21.5-1+cuda12.4_aarch64.txz'>nccl_2.21.5-1+cuda12.4_aarch64.txz</a>
        <a href='nccl_2.21.5-1+cuda11.0_ppc64le.txz'>nccl_2.21.5-1+cuda11.0_ppc64le.txz</a>
    </body></html>";

    #[test]
    fn scrape_versions_keeps_only_dotted_version_dirs() {
        let mut got = scrape_nccl_versions(NCCL_INDEX);
        got.sort();
        assert_eq!(got, vec!["2.20.5", "2.21.5", "2.30.7"]);
    }

    #[test]
    fn scrape_files_dedupes_href_and_text() {
        // Each file name appears twice (href + anchor text); expect one each.
        let got = scrape_nccl_files(NCCL_DIR_2215);
        assert_eq!(got.len(), 5, "five distinct archives: {got:?}");
        assert!(got.contains(&"nccl_2.21.5-1+cuda12.4_x86_64.txz".to_string()));
    }

    #[test]
    fn parse_filename_extracts_version_cuda_and_arch() {
        let a = parse_nccl_filename("nccl_2.21.5-1+cuda12.4_x86_64.txz").expect("parses");
        assert_eq!(a.nccl_version, "2.21.5");
        assert_eq!((a.cuda_major, a.cuda_minor), (12, 4));
        assert_eq!(a.arch, "x86_64");
        // arch with an underscore-free token
        let p = parse_nccl_filename("nccl_2.21.5-1+cuda11.0_ppc64le.txz").unwrap();
        assert_eq!(p.arch, "ppc64le");
        assert_eq!((p.cuda_major, p.cuda_minor), (11, 0));
        // non-NCCL / malformed names are rejected
        assert!(parse_nccl_filename("random.txz").is_none());
        assert!(parse_nccl_filename("nccl_2.21.5-1+cuda12.4_x86_64.tar.gz").is_none());
    }

    #[test]
    fn nccl_arch_maps_linux_and_rejects_windows() {
        use cuvm_core::{Arch, Os};
        let lin = Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        assert_eq!(nccl_arch(lin), Some("x86_64"));
        let arm = Platform {
            os: Os::Linux,
            arch: Arch::Aarch64,
        };
        assert_eq!(nccl_arch(arm), Some("aarch64"));
        let win = Platform {
            os: Os::Windows,
            arch: Arch::X86_64,
        };
        assert_eq!(nccl_arch(win), None);
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

    #[test]
    fn scrape_dedups_equal_versions_with_distinct_raw() {
        let html = r#"
          <a href="redistrib_13.3.json">a</a>
          <a href="redistrib_13.3.0.json">b</a>
        "#;
        let raws = super::scrape_redistrib_versions(html);
        // The scraper keeps both raw strings; Version Eq collapses them downstream.
        assert_eq!(raws, vec!["13.3", "13.3.0"]);
        let mut versions: Vec<cuvm_core::Version> = raws
            .iter()
            .map(|s| cuvm_core::Version::parse(s).unwrap())
            .collect();
        versions.sort();
        versions.dedup();
        assert_eq!(versions.len(), 1, "13.3 and 13.3.0 must collapse to one");
    }

    #[test]
    fn parse_accepts_string_sizes_like_the_production_redist() {
        // Real NVIDIA manifests serialize size as a JSON string
        // ("size": "1099680"); only our fixtures used numbers.
        let json = r#"{
            "release_date": "2024-03-01",
            "cuda_cudart": {
                "name": "CUDA Runtime",
                "version": "12.4.131",
                "linux-x86_64": {
                    "relative_path": "cuda_cudart/linux-x86_64/a.tar.xz",
                    "sha256": "ab",
                    "md5": "cd",
                    "size": "1099680"
                }
            }
        }"#;
        let m = RedistManifest::parse(json).expect("string sizes must parse");
        let art = m.component("cuda_cudart").unwrap().platforms["linux-x86_64"].clone();
        assert_eq!(art.size, 1_099_680);
    }
}

#[cfg(test)]
mod cudnn_manifest_tests {
    use super::*;

    /// Mirrors the live `redistrib_9.23.0.json` shape (per-product, nested
    /// cuda-variant platform maps, string sizes, 4-field product version).
    const CUDNN_9230: &str = r#"{
        "release_date": "2026-05-29",
        "release_label": "9.23.0",
        "release_product": "cudnn",
        "cudnn": {
            "name": "NVIDIA CUDA Deep Neural Network library",
            "license": "cudnn",
            "license_path": "cudnn/LICENSE.txt",
            "version": "9.23.0.39",
            "cuda_variant": ["12", "13"],
            "linux-x86_64": {
                "cuda12": {
                    "relative_path": "cudnn/linux-x86_64/cudnn-linux-x86_64-9.23.0.39_cuda12-archive.tar.xz",
                    "sha256": "7d2c",
                    "md5": "f33b",
                    "size": "967524328"
                },
                "cuda13": {
                    "relative_path": "cudnn/linux-x86_64/cudnn-linux-x86_64-9.23.0.39_cuda13-archive.tar.xz",
                    "sha256": "69eb",
                    "md5": "c12d",
                    "size": "897413624"
                }
            },
            "windows-x86_64": {
                "cuda12": {
                    "relative_path": "cudnn/windows-x86_64/cudnn-windows-x86_64-9.23.0.39_cuda12-archive.zip",
                    "sha256": "495f",
                    "md5": "57cd",
                    "size": "1817976536"
                }
            }
        },
        "cudnn_jit": {
            "name": "NVIDIA CUDA Deep Neural Network Graph JIT library",
            "license": "cudnn",
            "version": "9.23.0.39"
        }
    }"#;

    #[test]
    fn parse_extracts_the_cudnn_product_and_nested_variants() {
        let m = CudnnManifest::parse(CUDNN_9230).expect("parses");
        assert_eq!(m.product_version.as_deref(), Some("9.23.0.39"));
        // Metadata keys (name/license/license_path/cuda_variant) must not be
        // mistaken for platforms; only the two real platform maps survive.
        assert_eq!(m.platforms.len(), 2);
        let art = m.artifact("linux-x86_64", 12).expect("cuda12 build exists");
        assert!(art.relative_path.ends_with("_cuda12-archive.tar.xz"));
        assert_eq!(art.sha256, "7d2c");
        assert_eq!(art.size_bytes(), 967_524_328);
    }

    /// Mirrors the 8.x-era shape (`redistrib_8.9.7.json`): 3-field release
    /// label, ppc64le builds with `cuda11` variants, a `cudnn_samples`
    /// sibling product, and both size hazards (numeric size, absent size).
    const CUDNN_897: &str = r#"{
        "release_date": "2023-12-05",
        "release_label": "8.9.7",
        "release_product": "cudnn",
        "cudnn": {
            "name": "NVIDIA CUDA Deep Neural Network library",
            "license": "cudnn",
            "version": "8.9.7.29",
            "cuda_variant": ["11", "12"],
            "linux-ppc64le": {
                "cuda11": {
                    "relative_path": "cudnn/linux-ppc64le/cudnn-linux-ppc64le-8.9.7.29_cuda11-archive.tar.xz",
                    "sha256": "aa11",
                    "size": 12345
                },
                "cuda12": {
                    "relative_path": "cudnn/linux-ppc64le/cudnn-linux-ppc64le-8.9.7.29_cuda12-archive.tar.xz",
                    "sha256": "bb22"
                }
            },
            "linux-x86_64": {
                "cuda11": {
                    "relative_path": "cudnn/linux-x86_64/cudnn-linux-x86_64-8.9.7.29_cuda11-archive.tar.xz",
                    "sha256": "cc33",
                    "size": "843423789"
                }
            }
        },
        "cudnn_samples": {
            "name": "NVIDIA cuDNN samples",
            "license": "cudnn",
            "version": "8.9.7.29",
            "linux-x86_64": {
                "cuda11": {
                    "relative_path": "cudnn/linux-x86_64/cudnn_samples-linux-x86_64-8.9.7.29_cuda11-archive.tar.xz",
                    "sha256": "dd44",
                    "size": "1234"
                }
            }
        }
    }"#;

    #[test]
    fn eight_x_manifest_resolves_ppc64le_and_tolerates_numeric_or_absent_size() {
        let m = CudnnManifest::parse(CUDNN_897).expect("8.x manifest parses");
        assert_eq!(m.product_version.as_deref(), Some("8.9.7.29"));
        assert_eq!(
            m.platforms.len(),
            2,
            "`cudnn_samples` sibling product must not leak into platforms"
        );
        // A numeric `size` must not poison the whole platform (the
        // filter_map in `parse` would surface it as MissingPlatform).
        let art = m
            .artifact("linux-ppc64le", 11)
            .expect("cuda11 ppc64le build exists");
        assert!(art.relative_path.ends_with("_cuda11-archive.tar.xz"));
        assert_eq!(art.size, Some(12_345), "numeric size must parse");
        assert_eq!(art.size_bytes(), 12_345);
        let no_size = m
            .artifact("linux-ppc64le", 12)
            .expect("cuda12 build exists");
        assert_eq!(no_size.size, None);
        assert_eq!(no_size.size_bytes(), 0, "absent size defaults to 0");
    }

    #[test]
    fn missing_platform_and_missing_variant_are_distinct_errors() {
        let m = CudnnManifest::parse(CUDNN_9230).unwrap();
        let e1 = m.artifact("linux-sbsa", 12).unwrap_err();
        assert!(matches!(e1, RegistryError::MissingPlatform { .. }), "{e1}");
        let e2 = m.artifact("windows-x86_64", 13).unwrap_err();
        assert!(
            matches!(e2, RegistryError::MissingCudaVariant { .. }),
            "{e2}"
        );
        assert!(e2.to_string().contains("cuda13"), "{e2}");
    }

    #[test]
    fn manifest_without_a_cudnn_product_is_a_parse_error() {
        let err = CudnnManifest::parse(r#"{"release_label": "9.9.9"}"#).unwrap_err();
        assert!(matches!(err, RegistryError::Parse(_)), "{err}");
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
