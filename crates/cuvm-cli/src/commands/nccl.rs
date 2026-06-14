//! NCCL companion pipeline (spec §2.3, WU-20b): resolve target → acquire
//! (redist download with a self-recorded sha256, or a user-supplied archive) →
//! content-addressed store → link the full `libnccl*` set into the toolkit →
//! `.cuvm-nccl.json` sidecar.
//!
//! NCCL is BSD-licensed, so there is no EULA gate (unlike cuDNN). The NCCL
//! redist publishes no checksums, so downloads go through
//! [`cuvm_download::Downloader::fetch_unverified`] and cuvm hashes the bytes
//! itself; the resulting digest is the content-store key.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use cuvm_app::{Inventory, RegistryClient};
use cuvm_core::{current_platform, NcclRecord, Source, Version};
use cuvm_store::{nccl_store, Layout};

use crate::commands::cudnn::resolve_target;

/// Parse `nccl_<ver>-<build>+cuda<MAJOR.MINOR>_<arch>.txz` → `(version, cuda_major)`.
/// The standard redist archive name is the only user-supplied naming cuvm reads.
#[must_use]
pub fn parse_nccl_archive_name(name: &str) -> Option<(Version, u32)> {
    let stem = name.strip_prefix("nccl_")?.strip_suffix(".txz")?;
    // stem = "2.21.5-1+cuda12.4_x86_64"
    let (ver_build, cuda_arch) = stem.split_once("+cuda")?;
    let ver = ver_build.split_once('-').map_or(ver_build, |(v, _)| v);
    let (cuda, _arch) = cuda_arch.split_once('_')?;
    let major: u32 = cuda.split('.').next()?.parse().ok()?;
    let version = Version::parse(ver).ok()?;
    Some((version, major))
}

/// True when `want` is a numeric version-field prefix of `have` (`2.21` matches
/// `2.21.5`).
fn version_prefix_matches(want: &Version, have: &Version) -> bool {
    want.fields.len() <= have.fields.len()
        && want.fields.iter().zip(&have.fields).all(|(a, b)| a == b)
}

/// Pick the newest listed NCCL matching `spec` (`latest` ⇒ newest overall).
fn pick_listed(available: &[Version], spec: &str) -> Option<Version> {
    if spec == "latest" {
        return available.iter().max().cloned();
    }
    let want = Version::parse(spec).ok()?;
    available
        .iter()
        .filter(|v| version_prefix_matches(&want, v))
        .max()
        .cloned()
}

/// Extract a fetched/supplied NCCL `.txz` (tar.xz) into a staging dir under the
/// nccl store root, strip the wrapper, and publish content-addressed.
fn extract_into_store(layout: &Layout, archive: &Path, sha256: &str) -> Result<PathBuf> {
    let staged = layout.nccl_dir().join(format!(".stage-{sha256}"));
    if staged.exists() {
        std::fs::remove_dir_all(&staged)
            .with_context(|| format!("clearing stale staging {}", staged.display()))?;
    }
    std::fs::create_dir_all(&staged)
        .with_context(|| format!("creating staging {}", staged.display()))?;
    cuvm_download::extract_tar_xz(archive, &staged)
        .with_context(|| format!("extracting {}", archive.display()))?;
    cuvm_download::strip_wrapper_dir(&staged)
        .with_context(|| format!("stripping wrapper dir in {}", staged.display()))?;
    Ok(nccl_store::place_staged(layout, sha256, &staged)?)
}

/// Store→link→record tail shared by both acquisition paths.
fn store_link_record(
    layout: &Layout,
    target_root: &Path,
    archive: &Path,
    sha256: &str,
    version: &Version,
    cuda_major: u32,
    source: Source,
) -> Result<NcclRecord> {
    let os = current_platform().os;
    let store = extract_into_store(layout, archive, sha256)?;
    // Never unlink the existing NCCL until the replacement is known good: an
    // archive that parses but ships no libnccl* must fail HERE, leaving the
    // current pairing untouched.
    if nccl_store::lib_names(&store).is_empty() {
        bail!(
            "archive contained no libnccl* libraries (looked in lib/ and bin/ of {})",
            store.display()
        );
    }
    cuvm_platform::cudnn_link::unlink_nccl(os, target_root)?;
    let libs = cuvm_platform::cudnn_link::link_nccl(os, &store, target_root)?;
    if libs.is_empty() {
        bail!(
            "archive contained no libnccl* libraries (looked in lib/ and bin/ of {})",
            store.display()
        );
    }
    let record = NcclRecord {
        version: version.raw.clone(),
        cuda_major,
        source,
        sha256: sha256.to_string(),
        libs,
        installed_at: time::OffsetDateTime::now_utc(),
    };
    nccl_store::write_nccl_meta(target_root, &record)?;
    Ok(record)
}

/// Registry path: list → pick → resolve → download (unverified) → self-record
/// sha256 → tail. The NCCL build is selected for the toolkit's CUDA major.
fn install_from_registry(
    registry: &dyn RegistryClient,
    layout: &Layout,
    target_root: &Path,
    cuda_major: u32,
    spec: &str,
) -> Result<NcclRecord> {
    let platform = current_platform();
    let available = registry
        .list_nccl(&platform)
        .context("listing NCCL versions")?;
    let picked = pick_listed(&available, spec)
        .with_context(|| format!("no NCCL in the redist index matches `{spec}`"))?;

    let arts = registry
        .resolve_nccl(&picked, &platform, cuda_major)
        .with_context(|| format!("resolving NCCL {} for cuda{cuda_major}", picked.raw))?;
    let art = arts.first().context("registry returned no NCCL artifact")?;
    let file_name = art
        .relative_path
        .rsplit('/')
        .next()
        .unwrap_or(&art.relative_path)
        .to_string();

    let downloader = cuvm_download::Downloader::with_reporter(
        crate::composition::cache_dir(layout.root()),
        crate::reporter::CliReporter::shared(),
    );
    let label = format!("nccl {}", picked.raw);
    // No manifest checksums (spec §2.3): fetch unverified, then self-record.
    let archive = downloader
        .fetch_unverified(&art.url, &file_name, &label)
        .with_context(|| format!("downloading {file_name}"))?;
    let sha256 = cuvm_download::sha256_file(&archive)
        .with_context(|| format!("hashing {}", archive.display()))?;
    store_link_record(
        layout,
        target_root,
        &archive,
        &sha256,
        &picked,
        cuda_major,
        Source::Downloaded,
    )
}

/// User-supplied path: parse name → sha → tail. No network, no EULA.
fn install_from_file(
    layout: &Layout,
    target_root: &Path,
    toolkit_major: u32,
    target_handle: &str,
    file: &Path,
) -> Result<NcclRecord> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no file name", file.display()))?;
    let (version, cuda_major) = parse_nccl_archive_name(name).with_context(|| {
        format!(
            "`{name}` is not a standard NCCL redist archive name \
             (expected nccl_<ver>-<build>+cuda<X.Y>_<arch>.txz)"
        )
    })?;
    // NCCL builds are CUDA-major specific; refuse a mismatched pairing.
    if cuda_major != toolkit_major {
        bail!(
            "NCCL archive `{name}` is built for CUDA {cuda_major}, but {target_handle} is CUDA {toolkit_major}"
        );
    }
    let sha256 =
        cuvm_download::sha256_file(file).with_context(|| format!("hashing {}", file.display()))?;
    store_link_record(
        layout,
        target_root,
        file,
        &sha256,
        &version,
        cuda_major,
        Source::Supplied,
    )
}

/// `cuvm nccl install <ver|file> --for <toolkit>`.
///
/// # Errors
/// Returns an error when the target cannot be resolved (or is adopted), when no
/// matching NCCL exists, on a CUDA-major mismatch, or on any
/// download/extract/link/sidecar failure.
pub fn run_nccl_install(
    registry: &dyn RegistryClient,
    inventory: &dyn Inventory,
    home: &Path,
    what: &str,
    for_spec: &str,
) -> Result<i32> {
    let layout = Layout::new(home);
    let target = resolve_target(inventory, &layout, for_spec)?;
    let toolkit_major = target.toolkit_version.major();
    let path = Path::new(what);
    let record = if path.is_file() {
        install_from_file(&layout, &target.root, toolkit_major, &target.handle, path)?
    } else {
        install_from_registry(registry, &layout, &target.root, toolkit_major, what)?
    };
    println!(
        "+ nccl {} (cuda{})  ->  {}",
        record.version, record.cuda_major, target.handle
    );
    eprintln!(
        "{}",
        crate::reporter::dim(&format!(
            "Linked {} libraries into {}",
            record.libs.len(),
            target.root.display()
        ))
    );
    Ok(0)
}

/// `cuvm nccl ls`: paired NCCL records per bundle, then unreferenced store
/// payloads (mirrors `cudnn ls`).
///
/// # Errors
/// Returns an error when the manifest cannot be loaded.
pub fn run_nccl_ls(inventory: &dyn Inventory, home: &Path) -> Result<()> {
    let layout = Layout::new(home);
    let manifest = inventory.load()?;
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    let mut any = false;
    for b in &manifest.bundles {
        let root = layout.resolve_record_path(&b.path);
        if let Some(rec) = nccl_store::read_nccl_meta(&root) {
            referenced.insert(rec.sha256.clone());
            println!(
                "{} (cuda{})  {}  ->  {}",
                rec.version,
                rec.cuda_major,
                rec.sha256.get(..12).unwrap_or(&rec.sha256),
                b.version
            );
            any = true;
        }
    }
    if let Ok(entries) = std::fs::read_dir(layout.nccl_dir()) {
        for entry in entries.filter_map(std::result::Result::ok) {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') || referenced.contains(&name) {
                continue;
            }
            println!("{}  (unreferenced)", name.get(..12).unwrap_or(&name));
            any = true;
        }
    }
    if !any {
        println!("(no nccl payloads)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_name_parses_version_and_cuda_major() {
        let (v, major) =
            parse_nccl_archive_name("nccl_2.21.5-1+cuda12.4_x86_64.txz").expect("standard name");
        assert_eq!(v.raw, "2.21.5");
        assert_eq!(major, 12);
        let (v, major) = parse_nccl_archive_name("nccl_2.18.3-1+cuda11.0_aarch64.txz").unwrap();
        assert_eq!(v.raw, "2.18.3");
        assert_eq!(major, 11);
        assert!(parse_nccl_archive_name("random.txz").is_none());
        assert!(parse_nccl_archive_name("nccl_2.21.5-1+cuda12.4_x86_64.tar.gz").is_none());
    }

    fn available() -> Vec<Version> {
        ["2.18.3", "2.20.5", "2.21.5", "2.27.3"]
            .iter()
            .map(|s| Version::parse(s).expect("test version parses"))
            .collect()
    }

    #[test]
    fn pick_listed_latest_takes_newest_overall() {
        assert_eq!(pick_listed(&available(), "latest").unwrap().raw, "2.27.3");
    }

    #[test]
    fn pick_listed_prefix_matches_numeric_fields_not_string_prefix() {
        // A minor prefix matches by NUMERIC field, not string: "2.21" → 2.21.5.
        assert_eq!(pick_listed(&available(), "2.21").unwrap().raw, "2.21.5");
        assert_eq!(pick_listed(&available(), "2.20").unwrap().raw, "2.20.5");
        // "2.2" is minor==2 (not "starts with 2"), which nothing here satisfies.
        assert!(pick_listed(&available(), "2.2").is_none());
    }

    #[test]
    fn pick_listed_no_match_is_none() {
        assert!(pick_listed(&available(), "3.0").is_none());
    }
}
