//! cuDNN bundling pipeline (spec §10, plan D5–D8): EULA gate → acquire (redist
//! download or user-supplied archive) → extract → content-addressed store →
//! link into the toolkit → sidecar + manifest record.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use cuvm_app::{CompatEngine, Inventory, RegistryClient};
use cuvm_core::{current_platform, CudnnRecord, Source, Version};
use cuvm_store::cudnn_store;
use cuvm_store::eula;
use cuvm_store::Layout;

/// What the EULA gate decided (D7).
#[derive(Debug, PartialEq, Eq)]
pub enum EulaDecision {
    /// Go ahead; `record` is true when this run IS the acceptance moment.
    Proceed {
        /// Whether this run is the acceptance moment (write the EULA record).
        record: bool,
    },
    /// Do not download; the reason is user-facing.
    Refused {
        /// User-facing explanation of the refusal.
        reason: String,
    },
}

/// Pure EULA gate: recorded acceptance ∨ `--accept-eula` ∨ interactive yes.
/// `ask` is only invoked when interactive and needed (the acceptance moment).
pub fn eula_gate(
    already_accepted: bool,
    accept_flag: bool,
    interactive: bool,
    ask: impl FnOnce() -> bool,
) -> EulaDecision {
    if already_accepted {
        return EulaDecision::Proceed { record: false };
    }
    if accept_flag {
        return EulaDecision::Proceed { record: true };
    }
    if interactive && ask() {
        return EulaDecision::Proceed { record: true };
    }
    EulaDecision::Refused {
        reason: "the NVIDIA cuDNN EULA has not been accepted (cuvm never downloads silently); \
                 re-run with --accept-eula, or run interactively to accept once"
            .to_string(),
    }
}

/// Parse `cudnn-<platform>-<version>_cuda<major>-archive.<ext>` →
/// `(version, cuda_major)`. The standard redist archive name is the only
/// user-supplied naming cuvm understands (it carries the pairing facts).
#[must_use]
pub fn parse_cudnn_archive_name(name: &str) -> Option<(Version, u32)> {
    let stem = name
        .strip_suffix(".tar.xz")
        .or_else(|| name.strip_suffix(".zip"))?;
    let stem = stem.strip_suffix("-archive")?;
    let (rest, cuda) = stem.rsplit_once("_cuda")?;
    let major: u32 = cuda.parse().ok()?;
    // rest = cudnn-<platform-with-dashes>-<version>; version is the last '-' field.
    let (_, ver) = rest.rsplit_once('-')?;
    let version = Version::parse(ver).ok()?;
    Some((version, major))
}

/// The installed toolkit a cuDNN is being paired with.
pub struct Target {
    /// Manifest handle (the bundle's `version` string).
    pub handle: String,
    /// Absolute toolkit root the cuDNN libraries are linked into.
    pub root: PathBuf,
    /// How the toolkit got here (adopted ones are refused upstream, D8).
    pub source: Source,
    /// Parsed toolkit version (drives pairing and `cuda_major`).
    pub toolkit_version: Version,
}

/// Resolve `--for <spec>` to an installed bundle and refuse adopted ones (D8).
///
/// Matches the handle exactly OR by version-field prefix (`12.4` matches a
/// `12.4.1` bundle); the newest matching bundle wins.
///
/// # Errors
/// Returns an error when no installed toolkit matches `spec`, when the match
/// is an adopted install (cuvm never modifies adopted installs — ADR-005), or
/// when manifest I/O fails.
pub fn resolve_target(inventory: &dyn Inventory, layout: &Layout, spec: &str) -> Result<Target> {
    let manifest = inventory.load()?;
    let rec = manifest
        .bundles
        .iter()
        .filter(|b| {
            b.version == spec
                || Version::parse(&b.version).is_ok_and(|v| {
                    Version::parse(spec).is_ok_and(|s| {
                        s.fields.len() <= v.fields.len()
                            && s.fields.iter().zip(&v.fields).all(|(a, f)| a == f)
                    })
                })
        })
        .max_by_key(|b| Version::parse(&b.version).ok().map(|v| v.fields.clone()))
        .with_context(|| format!("no installed toolkit matches `{spec}`; run `cuvm ls`"))?;
    if matches!(rec.source, Source::Adopted) {
        bail!(
            "{} is adopted in place; cuvm never modifies adopted installs (ADR-005). \
             Install the toolkit with `cuvm install {}` first.",
            rec.version,
            rec.version
        );
    }
    Ok(Target {
        handle: rec.version.clone(),
        root: layout.resolve_record_path(&rec.path),
        source: rec.source,
        toolkit_version: Version::parse(&rec.version)
            .with_context(|| format!("manifest handle `{}` is not a version", rec.version))?,
    })
}

/// License URL shown at the acceptance moment (per-product, spec §2.3).
fn license_url() -> String {
    format!(
        "{}cudnn/LICENSE.txt",
        crate::composition::cudnn_registry_base_url()
    )
}

/// Run the EULA gate against the on-disk record + flags + TTY state, printing
/// the notice/prompt when interactive. Returns Ok(true) to proceed.
fn gate_and_maybe_record(layout: &Layout, accept_flag: bool) -> Result<bool> {
    let interactive = std::io::stderr().is_terminal() && std::io::stdin().is_terminal();
    let decision = eula_gate(eula::cudnn_accepted(layout), accept_flag, interactive, || {
        eprintln!(
            "cuDNN is distributed under the NVIDIA cuDNN EULA ({}).\n\
             Downloading it with cuvm means you accept those terms (recorded once under ~/.cuvm/eula/).",
            license_url()
        );
        eprint!("Accept and continue? [y/N] ");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap_or(0);
        matches!(line.trim(), "y" | "Y" | "yes")
    });
    match decision {
        EulaDecision::Proceed { record } => {
            if record {
                eula::record_cudnn_acceptance(
                    layout,
                    time::OffsetDateTime::now_utc(),
                    &license_url(),
                )?;
            }
            Ok(true)
        }
        EulaDecision::Refused { reason } => {
            eprintln!("cuvm: {reason}");
            Ok(false)
        }
    }
}

/// Extract a fetched/supplied cuDNN archive into a staging dir under the
/// cudnn store root, strip the wrapper, and publish content-addressed.
fn extract_into_store(layout: &Layout, archive: &Path, sha256: &str) -> Result<PathBuf> {
    let staged = layout.cudnn_dir().join(format!(".stage-{sha256}"));
    if staged.exists() {
        std::fs::remove_dir_all(&staged)
            .with_context(|| format!("clearing stale staging {}", staged.display()))?;
    }
    std::fs::create_dir_all(&staged)
        .with_context(|| format!("creating staging {}", staged.display()))?;
    let name = archive.to_string_lossy();
    if name.ends_with(".zip") {
        cuvm_download::extract_zip(archive, &staged)
    } else {
        cuvm_download::extract_tar_xz(archive, &staged)
    }
    .with_context(|| format!("extracting {}", archive.display()))?;
    cuvm_download::strip_wrapper_dir(&staged)
        .with_context(|| format!("stripping wrapper dir in {}", staged.display()))?;
    Ok(cudnn_store::place_staged(layout, sha256, &staged)?)
}

/// Store→link→record tail shared by every acquisition path.
#[allow(clippy::too_many_arguments)]
fn store_link_record(
    inventory: &dyn Inventory,
    layout: &Layout,
    target: &Target,
    archive: &Path,
    sha256: &str,
    version: &Version,
    cuda_major: u32,
    source: Source,
) -> Result<CudnnRecord> {
    let os = current_platform().os;
    let store = extract_into_store(layout, archive, sha256)?;
    cuvm_platform::cudnn_link::unlink_cudnn(os, &target.root)?;
    let libs = cuvm_platform::cudnn_link::link_cudnn(os, &store, &target.root)?;
    if libs.is_empty() {
        bail!(
            "archive contained no libcudnn* libraries (looked in lib/ and bin/ of {})",
            store.display()
        );
    }
    let record = CudnnRecord {
        version: version.raw.clone(),
        cuda_major,
        source,
        sha256: sha256.to_string(),
        libs,
        installed_at: time::OffsetDateTime::now_utc(),
    };
    cudnn_store::write_cudnn_meta(&target.root, &record)?;
    let mut manifest = inventory.load()?;
    for b in &mut manifest.bundles {
        if b.version == target.handle {
            b.cudnn = Some(version.raw.clone());
        }
    }
    inventory.save(&manifest)?;
    Ok(record)
}

/// Pick the newest listed cuDNN matching `spec` (`latest` ⇒ newest overall).
fn pick_listed(available: &[Version], spec: &str) -> Option<Version> {
    if spec == "latest" {
        return available.iter().max().cloned();
    }
    let want = Version::parse(spec).ok()?;
    available
        .iter()
        .filter(|v| {
            want.fields.len() <= v.fields.len()
                && want.fields.iter().zip(&v.fields).all(|(a, b)| a == b)
        })
        .max()
        .cloned()
}

/// Registry path: list → pick → validate pair → EULA → fetch → tail.
/// Returns Ok(None) when the EULA gate refused (caller decides severity, D7).
fn install_from_registry(
    registry: &dyn RegistryClient,
    engine: &dyn CompatEngine,
    inventory: &dyn Inventory,
    layout: &Layout,
    target: &Target,
    spec: &str,
    accept_eula: bool,
) -> Result<Option<CudnnRecord>> {
    let platform = current_platform();
    let cuda_major = target.toolkit_version.major();
    let available = registry
        .list_cudnn(&platform, cuda_major)
        .context("listing cuDNN versions")?;
    let picked = if spec == "default" {
        engine.pair_cudnn(&target.toolkit_version, &available)
    } else {
        pick_listed(&available, spec)
    }
    .with_context(|| format!("no cuDNN in the redist index matches `{spec}`"))?;

    let verdict = engine.validate_pair(&target.toolkit_version, &picked);
    if !verdict.ok {
        bail!("{}", verdict.reason);
    }
    if !gate_and_maybe_record(layout, accept_eula)? {
        return Ok(None);
    }

    let arts = registry
        .resolve_cudnn(&picked, &platform, cuda_major)
        .with_context(|| format!("resolving cuDNN {} for cuda{cuda_major}", picked.raw))?;
    let art = arts.first().context("registry returned no cuDNN artifact")?;
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
    let label = format!("cudnn {}", picked.raw);
    let archive = downloader
        .fetch_labeled(&art.url, &art.sha256, &file_name, &label)
        .with_context(|| format!("downloading {file_name}"))?;
    store_link_record(
        inventory,
        layout,
        target,
        &archive,
        &art.sha256,
        &picked,
        cuda_major,
        Source::Downloaded,
    )
    .map(Some)
}

/// User-supplied path: sha → parse name → validate pair → tail. No EULA gate
/// (the user already obtained the archive themselves, D7).
fn install_from_file(
    engine: &dyn CompatEngine,
    inventory: &dyn Inventory,
    layout: &Layout,
    target: &Target,
    file: &Path,
) -> Result<CudnnRecord> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no file name", file.display()))?;
    let (version, cuda_major) = parse_cudnn_archive_name(name).with_context(|| {
        format!(
            "`{name}` is not a standard cuDNN redist archive name \
             (expected cudnn-<platform>-<ver>_cuda<major>-archive.tar.xz/.zip)"
        )
    })?;
    let verdict = engine.validate_pair(&target.toolkit_version, &version);
    if !verdict.ok {
        bail!("{}", verdict.reason);
    }
    let sha256 = cuvm_download::sha256_file(file)
        .with_context(|| format!("hashing {}", file.display()))?;
    store_link_record(
        inventory,
        layout,
        target,
        file,
        &sha256,
        &version,
        cuda_major,
        Source::Supplied,
    )
}

/// `cuvm cudnn install <ver|file> --for <toolkit>`. EULA refusal here = hard
/// error (D7: an explicit install must not silently no-op).
///
/// # Errors
/// Returns an error when the target cannot be resolved (or is adopted, D8),
/// when no matching cuDNN exists, when the pairing is invalid, when the EULA
/// gate refuses, or on any download/extract/link/manifest failure.
pub fn run_cudnn_install(
    registry: &dyn RegistryClient,
    engine: &dyn CompatEngine,
    inventory: &dyn Inventory,
    home: &Path,
    what: &str,
    for_spec: &str,
    accept_eula: bool,
) -> Result<i32> {
    let layout = Layout::new(home);
    let target = resolve_target(inventory, &layout, for_spec)?;
    let path = Path::new(what);
    let record = if path.is_file() {
        install_from_file(engine, inventory, &layout, &target, path)?
    } else {
        install_from_registry(
            registry, engine, inventory, &layout, &target, what, accept_eula,
        )?
        .ok_or_else(|| anyhow::anyhow!("cuDNN EULA not accepted; nothing installed"))?
    };
    println!(
        "+ cudnn {} (cuda{})  ->  {}",
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

/// Default pairing inside `cuvm install` (D5): warn-and-continue semantics.
/// Returns the paired cuDNN version on success, `None` on EULA refusal (the
/// gate already printed why) or pairing failure (warned here).
pub fn pair_for_install(
    registry: &dyn RegistryClient,
    engine: &dyn CompatEngine,
    inventory: &dyn Inventory,
    layout: &Layout,
    target: &Target,
    explicit: Option<&str>,
    accept_eula: bool,
) -> Option<String> {
    let spec = explicit.unwrap_or("default");
    match install_from_registry(registry, engine, inventory, layout, target, spec, accept_eula) {
        Ok(Some(rec)) => Some(rec.version),
        Ok(None) => None, // EULA refusal: notice already printed by the gate
        Err(e) => {
            eprintln!("cuvm: warning: cuDNN pairing failed: {e:#}; continuing without cuDNN");
            None
        }
    }
}

/// `cuvm cudnn ls`: paired records per bundle, then unreferenced store payloads.
///
/// # Errors
/// Returns an error when the manifest cannot be loaded.
pub fn run_cudnn_ls(inventory: &dyn Inventory, home: &Path) -> Result<()> {
    let layout = Layout::new(home);
    let manifest = inventory.load()?;
    let mut referenced: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut any = false;
    for b in &manifest.bundles {
        if b.cudnn.is_none() {
            continue;
        }
        let root = layout.resolve_record_path(&b.path);
        if let Some(rec) = cudnn_store::read_cudnn_meta(&root) {
            referenced.insert(rec.sha256.clone());
            println!(
                "{} (cuda{})  {}  ->  {}",
                rec.version,
                rec.cuda_major,
                &rec.sha256[..rec.sha256.len().min(12)],
                b.version
            );
            any = true;
        }
    }
    if let Ok(entries) = std::fs::read_dir(layout.cudnn_dir()) {
        for entry in entries.filter_map(std::result::Result::ok) {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') || referenced.contains(&name) {
                continue;
            }
            println!("{}  (unreferenced)", &name[..name.len().min(12)]);
            any = true;
        }
    }
    if !any {
        println!("(no cudnn payloads)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eula_gate_orders_recorded_flag_prompt_refusal() {
        let no_ask = || panic!("must not prompt");
        assert_eq!(
            eula_gate(true, false, false, no_ask),
            EulaDecision::Proceed { record: false }
        );
        assert_eq!(
            eula_gate(false, true, false, no_ask),
            EulaDecision::Proceed { record: true }
        );
        assert_eq!(
            eula_gate(false, false, true, || true),
            EulaDecision::Proceed { record: true }
        );
        assert!(matches!(
            eula_gate(false, false, true, || false),
            EulaDecision::Refused { .. }
        ));
        assert!(matches!(
            eula_gate(false, false, false, no_ask),
            EulaDecision::Refused { .. }
        ));
    }

    #[test]
    fn archive_name_parses_version_and_cuda_major() {
        let (v, major) =
            parse_cudnn_archive_name("cudnn-linux-x86_64-9.8.0.87_cuda12-archive.tar.xz")
                .expect("standard name parses");
        assert_eq!(v.raw, "9.8.0.87");
        assert_eq!(major, 12);
        let (v, major) =
            parse_cudnn_archive_name("cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip").unwrap();
        assert_eq!(v.raw, "8.9.7.29");
        assert_eq!(major, 11);
        assert!(parse_cudnn_archive_name("random.tar.xz").is_none());
        assert!(parse_cudnn_archive_name("cudnn-linux-x86_64-9.8.0-archive.tar.xz").is_none());
    }
}
