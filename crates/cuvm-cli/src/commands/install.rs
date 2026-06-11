//! `cuvm install` / `cuvm ls-remote` / `cuvm uninstall` — the acquire pipeline,
//! plus the default cuDNN pairing that follows a placed toolkit (spec §10, M3).

use std::path::Path;

use anyhow::Result;

use cuvm_app::{
    AcquirePlan, CompatEngine, ComponentPolicy, DriverProbe, Installer, Inventory, RegistryClient,
    Severity,
};
use cuvm_core::manifest::BundleRecord;
use cuvm_core::{current_platform, Driver, GpuClass, Os, Source, Version, VersionMeta};
use cuvm_store::{read_meta, redist_cache, write_meta, Layout};

use super::cudnn;

/// Result of installing a single spec; drives the per-target change line and the
/// aggregate summary (§5.1/§5.4 of the spec).
#[derive(Debug)]
pub(crate) enum InstallOutcome {
    /// Freshly installed (no prior bundle for this handle).
    Installed {
        handle: String,
        path: std::path::PathBuf,
    },
    /// Re-installed over an existing bundle (`--reinstall`).
    Reinstalled {
        handle: String,
        path: std::path::PathBuf,
    },
    /// Already present and `--reinstall` not passed: a no-op.
    AlreadyPresent { handle: String },
    /// Windows-only: the download path degraded to read-only adopt (spec §2.2).
    #[cfg(not(unix))]
    Adopted {
        handle: String,
        path: std::path::PathBuf,
    },
}

/// cuDNN behavior for an install run (plan D5/D7).
pub struct CudnnOpts {
    /// `--cudnn <ver>` override; `None` ⇒ matrix default pairing.
    pub explicit: Option<String>,
    /// `--no-cudnn`.
    pub skip: bool,
    /// `--accept-eula`.
    pub accept_eula: bool,
}

/// Result of the driver-ceiling compat gate. Per §11/§2.4 the gate is advisory:
/// it only `Refused`s when the toolkit is incompatible **and** `--force` was not
/// passed. A missing driver is always `Proceed` ("driver unknown, build-only OK").
#[derive(Debug)]
pub enum GateOutcome {
    /// Compatible (or `--force`d / driver absent): the install may proceed.
    Proceed,
    /// Incompatible and `--force` was not passed: the install is refused.
    Refused {
        /// Why the toolkit was deemed incompatible with the driver.
        reason: String,
        /// Actionable hint (always mentions `--force`; may add a `cuda-compat` note).
        hint: String,
    },
}

/// Run the driver→toolkit ceiling check and decide whether to proceed.
///
/// Never hard-blocks: an exceeded ceiling becomes a refusal that `--force`
/// overrides, with a `cuda-compat` hint when the GPU class is forward-compat
/// eligible on Linux (data-center / Jetson / NGC-ready RTX).
#[must_use]
pub fn compat_gate(
    engine: &dyn CompatEngine,
    driver: &Driver,
    want: &Version,
    force: bool,
) -> GateOutcome {
    if !driver.present {
        return GateOutcome::Proceed;
    }
    let verdict = engine.check_toolkit(driver, want, false);
    if verdict.ok || matches!(verdict.severity, Severity::Ok) {
        return GateOutcome::Proceed;
    }
    let eligible = matches!(
        driver.gpu_class,
        GpuClass::DataCenter | GpuClass::Jetson | GpuClass::NgcReadyRtx
    );
    let compat_note = if verdict.forward_compat_possible && eligible {
        " This GPU class is cuda-compat eligible (Linux): a forward-compat package may raise the ceiling."
    } else {
        ""
    };
    if force {
        eprintln!(
            "cuvm: warning: {} (proceeding due to --force).{compat_note}",
            verdict.reason
        );
        return GateOutcome::Proceed;
    }
    GateOutcome::Refused {
        reason: verdict.reason,
        hint: format!("re-run with --force to install anyway. (cuda-compat){compat_note}"),
    }
}

/// `cuvm install <spec>...`: install each spec, continuing past per-target
/// failures, then print an aggregate summary. Returns the process exit code
/// (`0` = all installed or no-op; `1` = at least one target failed).
///
/// # Errors
/// Returns an error only for an unrecoverable failure before the per-target loop
/// (none today — per-target errors are caught and summarized).
#[allow(clippy::too_many_arguments)]
pub fn run_install(
    registry: &dyn RegistryClient,
    installer: &dyn Installer,
    inventory: &dyn Inventory,
    engine: &dyn CompatEngine,
    driver_probe: &dyn DriverProbe,
    home: &Path,
    specs: &[String],
    reinstall: bool,
    force: bool,
    cudnn_opts: &CudnnOpts,
) -> Result<i32> {
    let started = std::time::Instant::now();
    let mut changed: Vec<String> = Vec::new();
    let mut failed = 0usize;

    for spec in specs {
        match install_one(
            registry,
            installer,
            inventory,
            engine,
            driver_probe,
            home,
            spec,
            reinstall,
            force,
            cudnn_opts,
        ) {
            Ok(InstallOutcome::Installed { handle, path }) => {
                println!("+ cuda {handle}  ->  {}", path.display());
                changed.push(handle);
            }
            Ok(InstallOutcome::Reinstalled { handle, path }) => {
                println!("~ cuda {handle}  ->  {}", path.display());
                changed.push(handle);
            }
            #[cfg(not(unix))]
            Ok(InstallOutcome::Adopted { handle, path }) => {
                println!("+ cuda {handle} (adopted)  ->  {}", path.display());
                changed.push(handle);
            }
            Ok(InstallOutcome::AlreadyPresent { handle }) => {
                eprintln!("cuvm: {handle} is already installed");
            }
            Err(e) => {
                eprintln!("cuvm: error installing {spec}: {e:#}");
                failed += 1;
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    match changed.len() {
        0 => {
            if failed == 0 && specs.len() > 1 {
                eprintln!("cuvm: all requested versions already installed");
            }
        }
        // Spec §5.4: the aggregate summary goes to stderr, dimmed (plain when
        // stderr is not a TTY); the idempotency notices above stay undimmed.
        1 => eprintln!(
            "{}",
            crate::reporter::dim(&format!("Installed CUDA {} in {elapsed:.1}s", changed[0]))
        ),
        n => eprintln!(
            "{}",
            crate::reporter::dim(&format!("Installed {n} toolkits in {elapsed:.1}s"))
        ),
    }
    Ok(i32::from(failed > 0))
}

/// Install a single resolved spec: resolve newest patch, short-circuit if already
/// installed (unless `reinstall`), compat-gate, acquire, verify, extract, place,
/// smoke-test, pair the default cuDNN (unless `--no-cudnn`; warn-and-continue),
/// then record a `Downloaded` manifest bundle. Returns the
/// [`InstallOutcome`] (`AlreadyPresent` / `Installed` / `Reinstalled`, or
/// `Adopted` on the Windows degrade path) for the caller to render; it prints
/// nothing itself.
///
/// # Errors
/// Returns an error if resolution, the compat gate (without `--force`), download,
/// verification, extraction, placement, the smoke test, or manifest I/O fails.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // flat pipeline: resolve → gate → acquire → place → pair → record
fn install_one(
    registry: &dyn RegistryClient,
    installer: &dyn Installer,
    inventory: &dyn Inventory,
    engine: &dyn CompatEngine,
    driver_probe: &dyn DriverProbe,
    home: &Path,
    spec: &str,
    reinstall: bool,
    force: bool,
    cudnn_opts: &CudnnOpts,
) -> Result<InstallOutcome> {
    let platform = current_platform();
    let version_dir = home.join("versions");

    // 1. Resolve newest patch matching `spec` from the registry.
    let mut available = registry.list_toolkits(&platform)?;
    available.sort();

    // Warm the redist-index cache after a successful live fetch (§6.2): a later
    // `cuvm ls` then renders the available rows offline. Best-effort — a
    // cache-write failure must never fail the install.
    let layout = Layout::new(home);
    let _ = redist_cache::write(
        &layout,
        &platform,
        &available,
        time::OffsetDateTime::now_utc(),
    );

    let want = available
        .iter()
        .rev()
        .find(|v| version_matches(spec, v))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no remote toolkit matches `{spec}`"))?;

    // Idempotency: a bundle already exists for this handle (any source).
    let existed = {
        let manifest = inventory.load()?;
        manifest.bundles.iter().any(|b| b.version == want.raw)
    };
    if existed && !reinstall {
        return Ok(InstallOutcome::AlreadyPresent {
            handle: want.raw.clone(),
        });
    }

    // 2. Driver-ceiling compat gate (warn + --force; never hard-block).
    let driver = driver_probe.probe()?;
    if let GateOutcome::Refused { reason, hint } = compat_gate(engine, &driver, &want, force) {
        anyhow::bail!("{reason}\nhint: {hint}");
    }

    // 3. Resolve component artifacts and build the acquire plan.
    let artifacts = registry.resolve_toolkit(&want, &platform, &ComponentPolicy::Recommended)?;
    let handle = want.raw.clone();
    let plan = AcquirePlan {
        artifacts,
        dest_handle: handle.clone(),
    };

    // 4. acquire -> verify -> extract(tmp) -> place(dst) -> smoke_test.
    //
    // Windows degrade-to-adopt handoff (spec §2.2): when the resolved windows-x86_64
    // component set is empty (CUDA >= 13.0 is Windows-N/A) or a download is blocked
    // by enterprise lockdown, fall back to the M1 adopt path rather than hard-failing.
    // The Linux ship-gate e2e never exercises this; a Windows-runner test is WU-19.
    #[cfg(not(unix))]
    if plan.artifacts.is_empty() {
        return degrade_to_adopt(
            installer,
            inventory,
            &handle,
            &format!("no windows-x86_64 redist components resolved for {handle}"),
        );
    }
    let cached = match installer.acquire(&plan) {
        Ok(c) => c,
        #[cfg(not(unix))]
        Err(e) => {
            return degrade_to_adopt(
                installer,
                inventory,
                &handle,
                &format!("windows download blocked, degrading to adopt-only: {e}"),
            );
        }
        #[cfg(unix)]
        Err(e) => return Err(e),
    };
    installer.verify(&cached)?;

    let dst = version_dir.join(&handle);
    let tmp = version_dir.join(format!(".tmp-{handle}"));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    let extracted = installer.extract_atomic(&cached, &tmp)?;

    let components: Vec<String> = plan.artifacts.iter().map(|a| a.component.clone()).collect();
    let installed_at = time::OffsetDateTime::now_utc();
    let meta = VersionMeta {
        version: handle.clone(),
        source: Source::Downloaded,
        cudnn: None,
        components: components.clone(),
        sha256: None,
        has_lib64: matches!(platform.os, Os::Linux),
        installed_at,
    };
    installer.place(&extracted, &dst, &meta)?;

    if std::env::var_os("CUVM_SKIP_SMOKE").is_none() {
        installer.smoke_test(&dst)?;
    }

    // 5. cuDNN default pairing (spec §10, plan D5): post-place, never fails
    // the toolkit install — refusals/errors warn and record no cudnn.
    let cudnn_version = if cudnn_opts.skip {
        None
    } else {
        let target = cudnn::Target {
            handle: handle.clone(),
            root: dst.clone(),
            source: Source::Downloaded,
            toolkit_version: want.clone(),
        };
        cudnn::pair_for_install(
            registry,
            engine,
            inventory,
            &layout,
            &target,
            cudnn_opts.explicit.as_deref(),
            cudnn_opts.accept_eula,
        )
    };

    // 6. Record a Downloaded bundle (path is `versions/<handle>`, relative to home).
    let mut manifest = inventory.load()?;
    let record = BundleRecord {
        version: handle.clone(),
        source: Source::Downloaded,
        path: format!("versions/{handle}"),
        cudnn: cudnn_version.clone(),
        components,
        sha256: None,
        installed_at,
    };
    manifest.bundles.retain(|b| b.version != record.version);
    manifest.bundles.push(record);
    inventory.save(&manifest)?;

    if cudnn_version.is_some() {
        // Mirror into the toolkit sidecar (read-modify-write keeps the other
        // fields the installer wrote; best-effort like the rich sidecar).
        let meta_path = dst.join(".cuvm-meta.json");
        if let Ok(mut meta) = read_meta(&meta_path) {
            meta.cudnn = cudnn_version;
            let _ = write_meta(&meta_path, &meta);
        }
    }

    if existed {
        Ok(InstallOutcome::Reinstalled { handle, path: dst })
    } else {
        Ok(InstallOutcome::Installed { handle, path: dst })
    }
}

/// `cuvm uninstall <ver>`: for `Downloaded`/`Supplied` rows, delete the
/// `versions/<ver>` directory and deregister; for `Adopted` rows, deregister only
/// (referenced-in-place files are never deleted — ADR-005).
///
/// # Errors
/// Returns an error if manifest I/O or directory removal fails.
pub fn run_uninstall(inventory: &dyn Inventory, home: &Path, spec: &str) -> Result<()> {
    let manifest = inventory.load()?;
    let row = manifest.bundles.iter().find(|b| b.version == spec).cloned();

    match row {
        Some(r) if matches!(r.source, Source::Downloaded | Source::Supplied) => {
            let dir = if Path::new(&r.path).is_absolute() {
                std::path::PathBuf::from(&r.path)
            } else {
                home.join(&r.path)
            };
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
            inventory.deregister(spec)?;
            println!("removed {spec}");
        }
        Some(_) => {
            inventory.deregister(spec)?;
            println!("deregistered {spec} (adopted files left in place)");
        }
        None => {
            inventory.deregister(spec)?;
            println!("deregistered {spec}");
        }
    }
    Ok(())
}

/// Windows-only: degrade a blocked/empty download into the M1 read-only adopt
/// path. Scans for an in-place toolkit matching the wanted handle and records an
/// `Adopted` bundle (referenced-in-place, never deleted — ADR-005).
#[cfg(not(unix))]
fn degrade_to_adopt(
    installer: &dyn Installer,
    inventory: &dyn Inventory,
    handle: &str,
    reason: &str,
) -> Result<InstallOutcome> {
    eprintln!("cuvm: warning: {reason}; falling back to adopt-only.");
    let candidates = installer.scan()?;
    let candidate = candidates
        .into_iter()
        .find(|c| adopt_candidate_matches(handle, &c.version))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot download {handle} and no matching in-place toolkit found to adopt"
            )
        })?;
    let bundle = installer.adopt(&candidate)?;
    let mut manifest = inventory.load()?;
    let record = BundleRecord {
        version: bundle.toolkit.version.raw.clone(),
        source: Source::Adopted,
        path: bundle.toolkit.root.display().to_string(),
        cudnn: None,
        components: bundle.toolkit.components.clone(),
        sha256: bundle.toolkit.checksum.clone(),
        installed_at: bundle.toolkit.installed_at,
    };
    manifest.bundles.retain(|b| b.version != record.version);
    manifest.bundles.push(record);
    inventory.save(&manifest)?;
    Ok(InstallOutcome::Adopted {
        handle: bundle.toolkit.version.raw.clone(),
        path: bundle.toolkit.root,
    })
}

/// Whether a scanned adopt `candidate` satisfies the registry-resolved `handle`
/// (always full `X.Y.Z`). Windows scan candidates come from `v<major>.<minor>`
/// dirs, so they usually carry only two fields; a candidate matches when **all
/// of its fields** equal the handle's leading fields (`12.4` matches `12.4.1`;
/// `12.4.0` does not — its third field disagrees; `12.4.1` matches itself).
///
/// Compiled (and unit-tested) on every platform so the matching rule stays
/// test-locked on the Linux lane; the only caller is the `cfg(not(unix))`
/// degrade-to-adopt path.
#[cfg_attr(unix, allow(dead_code))]
fn adopt_candidate_matches(handle: &str, candidate: &Version) -> bool {
    let Ok(want) = Version::parse(handle) else {
        return false;
    };
    !candidate.fields.is_empty()
        && candidate.fields.len() <= want.fields.len()
        && candidate
            .fields
            .iter()
            .zip(&want.fields)
            .all(|(c, w)| c == w)
}

/// Whether `version` satisfies `spec` (exact `X.Y.Z`, minor `X.Y`, major `X`, or
/// `latest`). The caller iterates newest-first, so the first match is the newest.
fn version_matches(spec: &str, version: &Version) -> bool {
    if spec == "latest" {
        return true;
    }
    let want: Vec<&str> = spec.split('.').collect();
    let have: Vec<String> = version.fields.iter().map(ToString::to_string).collect();
    if want.len() > have.len() {
        return false;
    }
    want.iter().zip(have.iter()).all(|(w, h)| w == h)
}

#[cfg(test)]
mod adopt_match_tests {
    use super::adopt_candidate_matches;
    use cuvm_core::Version;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn two_field_windows_candidate_matches_the_resolved_patch_handle() {
        // Windows scan candidates come from `v12.4` dirs; the registry handle is
        // always `X.Y.Z` — major.minor must be enough to adopt.
        assert!(adopt_candidate_matches("12.4.1", &v("12.4")));
    }

    #[test]
    fn exact_three_field_candidate_still_matches() {
        assert!(adopt_candidate_matches("12.4.1", &v("12.4.1")));
    }

    #[test]
    fn different_minor_does_not_match() {
        assert!(!adopt_candidate_matches("12.4.1", &v("12.6")));
    }

    #[test]
    fn three_field_candidate_must_agree_on_every_field() {
        // A fully-versioned scanned toolkit is matched exactly: 12.4.0 is a
        // different patch than the wanted 12.4.1.
        assert!(!adopt_candidate_matches("12.4.1", &v("12.4.0")));
    }

    #[test]
    fn different_major_does_not_match() {
        assert!(!adopt_candidate_matches("13.0.1", &v("12.4")));
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use cuvm_app::{CompatEngine, Severity, Verdict};
    use cuvm_core::{Arch, Driver, GpuClass, Os, Platform, Version};
    use mockall::mock;

    mock! {
        pub Eng {}
        impl CompatEngine for Eng {
            fn max_toolkit_for_driver(&self, d: &Driver) -> anyhow::Result<Version>;
            fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict;
            fn pair_cudnn(&self, tk: &Version, avail: &[Version]) -> Option<Version>;
            fn validate_pair(&self, tk: &Version, cudnn: &Version) -> Verdict;
        }
    }

    fn driver(present: bool, gpu: GpuClass) -> Driver {
        Driver {
            present,
            version: Version::parse("550.54.14").unwrap(),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            gpu_class: gpu,
        }
    }

    fn warn_verdict(fwd: bool) -> Verdict {
        Verdict {
            ok: false,
            severity: Severity::Warn,
            reason: "toolkit exceeds driver ceiling".into(),
            forward_compat_possible: fwd,
        }
    }

    #[test]
    fn ok_verdict_proceeds() {
        let mut eng = MockEng::new();
        eng.expect_check_toolkit().returning(|_, _, _| Verdict {
            ok: true,
            severity: Severity::Ok,
            reason: "within ceiling".into(),
            forward_compat_possible: false,
        });
        let want = Version::parse("12.4.1").unwrap();
        let out = compat_gate(&eng, &driver(true, GpuClass::GeForce), &want, false);
        assert!(matches!(out, GateOutcome::Proceed));
    }

    #[test]
    fn warn_without_force_refuses_with_hint() {
        let mut eng = MockEng::new();
        eng.expect_check_toolkit()
            .returning(|_, _, _| warn_verdict(true));
        let want = Version::parse("12.9.0").unwrap();
        let out = compat_gate(&eng, &driver(true, GpuClass::DataCenter), &want, false);
        match out {
            GateOutcome::Refused { reason, hint } => {
                assert!(reason.contains("driver ceiling"));
                assert!(hint.contains("--force"));
                assert!(hint.contains("cuda-compat"));
            }
            GateOutcome::Proceed => panic!("expected refusal without --force"),
        }
    }

    #[test]
    fn warn_with_force_proceeds() {
        let mut eng = MockEng::new();
        eng.expect_check_toolkit()
            .returning(|_, _, _| warn_verdict(false));
        let want = Version::parse("12.9.0").unwrap();
        let out = compat_gate(&eng, &driver(true, GpuClass::GeForce), &want, true);
        assert!(matches!(out, GateOutcome::Proceed));
    }

    #[test]
    fn absent_driver_proceeds_without_consulting_engine() {
        let eng = MockEng::new(); // no expectations: must not be called
        let want = Version::parse("12.4.1").unwrap();
        let out = compat_gate(&eng, &driver(false, GpuClass::Unknown), &want, false);
        assert!(matches!(out, GateOutcome::Proceed));
    }
}
