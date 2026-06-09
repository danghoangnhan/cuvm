//! `cuvm install` / `cuvm ls-remote` / `cuvm uninstall` — the M2 acquire pipeline.
//!
//! `--cudnn`/`--no-cudnn` parse here but are **no-ops in M2**: cuDNN pairing is M3.

use anyhow::Result;

use cuvm_app::{CompatEngine, RegistryClient, Severity};
use cuvm_core::{current_platform, Driver, GpuClass, Version};

/// `cuvm ls-remote`: print available toolkit versions, newest first.
///
/// # Errors
/// Returns an error if the registry index cannot be fetched or parsed.
pub fn run_ls_remote(registry: &dyn RegistryClient) -> Result<()> {
    let platform = current_platform();
    let mut versions = registry.list_toolkits(&platform)?;
    versions.sort();
    versions.reverse();
    if versions.is_empty() {
        println!("(no remote toolkits found)");
        return Ok(());
    }
    for v in &versions {
        println!("{v}");
    }
    Ok(())
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
