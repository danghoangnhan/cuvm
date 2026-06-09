//! `doctor` v1 use-case: pure diagnostics over an already-probed environment.
//! No I/O here — the CLI reads env strings and the driver/bundle, and passes them in.

use std::fmt;

use crate::{CompatEngine, Severity};
use cuvm_core::{Driver, GpuClass, Version};

/// One diagnostic line. `code` is a stable machine-readable id (e.g. `DRIVER_CEILING`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: Option<String>,
}

/// The full ordered set of findings produced by one `doctor` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub findings: Vec<Finding>,
}

impl DoctorReport {
    /// Machine-readable exit code: 0 = all Ok, 1 = at least one Warn (no Block),
    /// 2 = at least one Block. Gates CI.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        let mut worst = 0i32;
        for f in &self.findings {
            let level = match f.severity {
                Severity::Ok => 0,
                Severity::Warn => 1,
                Severity::Block => 2,
            };
            if level > worst {
                worst = level;
            }
        }
        worst
    }

    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.exit_code() == 0
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Severity::Ok => "OK",
            Severity::Warn => "WARN",
            Severity::Block => "BLOCK",
        };
        f.write_str(s)
    }
}

impl fmt::Display for DoctorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for finding in &self.findings {
            writeln!(
                f,
                "[{}] {} ({})",
                finding.severity, finding.title, finding.code
            )?;
            writeln!(f, "    {}", finding.detail)?;
            if let Some(hint) = &finding.hint {
                writeln!(f, "    hint: {hint}")?;
            }
        }
        write!(f, "exit: {}", self.exit_code())
    }
}

/// Driver→toolkit ceiling diagnostic. Never blocks (§11): an exceeded ceiling is a
/// Warn (with a --force/cuda-compat path), and a missing driver is "build-only OK".
pub fn check_driver_ceiling(
    engine: &dyn CompatEngine,
    driver: &Driver,
    active: Option<&Version>,
) -> Finding {
    if !driver.present {
        return Finding {
            code: "DRIVER_ABSENT".into(),
            severity: Severity::Warn,
            title: "GPU driver not detected".into(),
            detail: "nvidia-smi reported no driver; driver unknown, build-only OK.".into(),
            hint: Some(
                "Install an NVIDIA driver to run CUDA programs; compilation still works.".into(),
            ),
        };
    }

    let ceiling = match engine.max_toolkit_for_driver(driver) {
        Ok(v) => v,
        Err(e) => {
            return Finding {
                code: "DRIVER_CEILING".into(),
                severity: Severity::Warn,
                title: "Could not determine driver ceiling".into(),
                detail: format!(
                    "driver {} ({}): {e}",
                    driver.version.raw, "ceiling lookup failed"
                ),
                hint: None,
            };
        }
    };

    let Some(active) = active else {
        return Finding {
            code: "DRIVER_CEILING".into(),
            severity: Severity::Ok,
            title: "Driver toolkit ceiling".into(),
            detail: format!(
                "driver {} supports CUDA up to {}; no toolkit is active.",
                driver.version.raw, ceiling.raw
            ),
            hint: None,
        };
    };

    let verdict = engine.check_toolkit(driver, active, false);
    if verdict.ok {
        return Finding {
            code: "DRIVER_CEILING".into(),
            severity: Severity::Ok,
            title: "Active toolkit within driver ceiling".into(),
            detail: format!(
                "active CUDA {} <= driver ceiling {} (driver {}).",
                active.raw, ceiling.raw, driver.version.raw
            ),
            hint: None,
        };
    }

    let eligible = matches!(
        driver.gpu_class,
        GpuClass::DataCenter | GpuClass::Jetson | GpuClass::NgcReadyRtx
    );
    let hint = if verdict.forward_compat_possible && eligible {
        Some(
            "This GPU class is cuda-compat eligible (Linux): a forward-compat package may raise \
             the ceiling."
                .into(),
        )
    } else {
        Some("Re-run with --force to proceed, or switch to a toolkit within the ceiling.".into())
    };

    Finding {
        code: "DRIVER_CEILING".into(),
        severity: verdict.severity,
        title: "Active toolkit exceeds driver ceiling".into(),
        detail: format!(
            "active CUDA {} exceeds driver ceiling {} (driver {}).",
            active.raw, ceiling.raw, driver.version.raw
        ),
        hint,
    }
}

/// Pre-read environment, passed in by the CLI so this module stays I/O-free.
/// `path_sep` is ':' on unix and ';' on windows (the CLI fills it from the OS).
#[derive(Debug, Clone)]
pub struct EnvSnapshot {
    pub path: String,
    pub ld_library_path: String,
    pub cuda_home: Option<String>,
    pub active_root: Option<String>,
    pub path_sep: char,
}

fn norm(p: &str) -> String {
    p.trim_end_matches(['/', '\\']).replace('\\', "/")
}

/// A segment is a "CUDA bin" if it ends in /bin and any component holds a "cuda" token.
fn is_cuda_bin(seg: &str) -> bool {
    let n = norm(seg).to_lowercase();
    n.ends_with("/bin") && n.contains("cuda")
}

fn is_cuda_lib(seg: &str) -> bool {
    let n = norm(seg).to_lowercase();
    (n.ends_with("/lib64") || n.ends_with("/lib")) && n.contains("cuda")
}

/// A bin/lib that lives under the active toolkit root counts as a CUDA dir even
/// when its path carries no literal "cuda" token: cuvm-managed toolkits live at
/// `~/.cuvm/versions/<ver>/{bin,lib64}`, which the token heuristic would miss.
fn under_active_with_suffix(seg: &str, active: Option<&str>, suffixes: &[&str]) -> bool {
    let Some(active) = active else {
        return false;
    };
    let n = norm(seg);
    let nl = n.to_lowercase();
    suffixes.iter().any(|suf| nl.ends_with(suf)) && n.starts_with(active)
}

/// `PATH`/`LD_LIBRARY_PATH` hygiene: dup CUDA dirs, stale CUDA entries, nvcc vs `CUDA_HOME`.
#[must_use]
pub fn check_path_hygiene(s: &EnvSnapshot) -> Vec<Finding> {
    let mut out = Vec::new();
    let active = s.active_root.as_deref().map(norm);

    let bins: Vec<&str> = s
        .path
        .split(s.path_sep)
        .filter(|seg| {
            !seg.is_empty()
                && (is_cuda_bin(seg) || under_active_with_suffix(seg, active.as_deref(), &["/bin"]))
        })
        .collect();

    // (a) duplicate CUDA bin dirs on PATH.
    if bins.len() > 1 {
        out.push(Finding {
            code: "PATH_DUP_CUDA".into(),
            severity: Severity::Warn,
            title: "Multiple CUDA bin directories on PATH".into(),
            detail: format!(
                "PATH contains {} CUDA bin entries: {}",
                bins.len(),
                bins.join(", ")
            ),
            hint: Some("Run `cuvm use <ver>` to rebuild a clean CUVM_INJECTED segment.".into()),
        });
    }

    // (b) stale CUDA bins that do not belong to the active root.
    if let Some(active) = &active {
        let stale: Vec<&&str> = bins
            .iter()
            .filter(|b| !norm(b).starts_with(active.as_str()))
            .collect();
        if !stale.is_empty() {
            out.push(Finding {
                code: "PATH_STALE_CUDA".into(),
                severity: Severity::Warn,
                title: "Stale CUDA bin on PATH".into(),
                detail: format!(
                    "PATH has CUDA bin(s) outside the active toolkit {active}: {}",
                    stale.iter().map(|x| **x).collect::<Vec<_>>().join(", ")
                ),
                hint: Some(
                    "`cuvm use <ver>` strips CUVM_INJECTED precisely; remove manual PATH edits."
                        .into(),
                ),
            });
        }
    }

    // (c) nvcc resolution vs CUDA_HOME: the FIRST CUDA bin on PATH is where nvcc resolves.
    if let (Some(home), Some(first_bin)) = (s.cuda_home.as_deref().map(norm), bins.first()) {
        let resolved_root = norm(first_bin)
            .strip_suffix("/bin")
            .map(str::to_string)
            .unwrap_or_default();
        if !resolved_root.is_empty() && resolved_root != home {
            out.push(Finding {
                code: "NVCC_MISMATCH".into(),
                severity: Severity::Block,
                title: "nvcc does not match CUDA_HOME".into(),
                detail: format!(
                    "nvcc resolves to {first_bin} (root {resolved_root}) but CUDA_HOME={home}; \
                     builds will use a different toolkit than the active one."
                ),
                hint: Some("Run `cuvm use <ver>` so the active bin is first on PATH.".into()),
            });
        }
    }

    // LD hygiene: dup CUDA lib dirs (mirrors PATH dup; warn only).
    let libs: Vec<&str> = s
        .ld_library_path
        .split(s.path_sep)
        .filter(|seg| {
            !seg.is_empty()
                && (is_cuda_lib(seg)
                    || under_active_with_suffix(seg, active.as_deref(), &["/lib64", "/lib"]))
        })
        .collect();
    if libs.len() > 1 {
        out.push(Finding {
            code: "LD_DUP_CUDA".into(),
            severity: Severity::Warn,
            title: "Multiple CUDA lib directories on LD_LIBRARY_PATH".into(),
            detail: format!(
                "LD_LIBRARY_PATH has {} CUDA lib entries: {}",
                libs.len(),
                libs.join(", ")
            ),
            hint: Some(
                "`cuvm use <ver>` strips the recorded CUVM_INJECTED lib segment first.".into(),
            ),
        });
    }

    if out.is_empty() {
        out.push(Finding {
            code: "PATH_HYGIENE".into(),
            severity: Severity::Ok,
            title: "PATH / LD_LIBRARY_PATH clean".into(),
            detail: "no duplicate or stale CUDA entries; nvcc matches CUDA_HOME.".into(),
            hint: None,
        });
    }
    out
}

/// Compose every v1 diagnostic into one ordered report. Pure: all inputs are pre-read.
pub fn run_doctor(
    engine: &dyn CompatEngine,
    driver: &Driver,
    active: Option<&Version>,
    env: &EnvSnapshot,
) -> DoctorReport {
    let mut findings = Vec::new();
    findings.push(check_driver_ceiling(engine, driver, active));
    findings.extend(check_path_hygiene(env));
    DoctorReport { findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severity;

    fn finding(sev: Severity) -> Finding {
        Finding {
            code: "X".into(),
            severity: sev,
            title: "t".into(),
            detail: "d".into(),
            hint: None,
        }
    }

    #[test]
    fn exit_code_is_zero_when_all_ok() {
        let r = DoctorReport {
            findings: vec![finding(Severity::Ok), finding(Severity::Ok)],
        };
        assert_eq!(r.exit_code(), 0);
        assert!(r.is_healthy());
    }

    #[test]
    fn exit_code_is_one_when_worst_is_warn() {
        let r = DoctorReport {
            findings: vec![finding(Severity::Ok), finding(Severity::Warn)],
        };
        assert_eq!(r.exit_code(), 1);
        assert!(!r.is_healthy());
    }

    #[test]
    fn exit_code_is_two_when_any_block_even_with_warns() {
        let r = DoctorReport {
            findings: vec![
                finding(Severity::Warn),
                finding(Severity::Block),
                finding(Severity::Ok),
            ],
        };
        assert_eq!(r.exit_code(), 2);
    }

    #[test]
    fn empty_report_is_healthy() {
        let r = DoctorReport { findings: vec![] };
        assert_eq!(r.exit_code(), 0);
        assert!(r.is_healthy());
    }
}

#[cfg(test)]
mod ceiling_tests {
    use super::*;
    use crate::{CompatEngine, Severity, Verdict};
    use cuvm_core::{Arch, Driver, GpuClass, Os, Platform, Version};
    use mockall::mock;

    mock! {
        pub Compat {}
        impl CompatEngine for Compat {
            fn max_toolkit_for_driver(&self, d: &Driver) -> anyhow::Result<Version>;
            fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict;
            fn pair_cudnn(&self, toolkit: &Version, available: &[Version]) -> Option<Version>;
            fn validate_pair(&self, toolkit: &Version, cudnn: &Version) -> Verdict;
        }
    }

    fn linux_driver(v: &str, present: bool, gpu: GpuClass) -> Driver {
        Driver {
            present,
            version: Version::parse(v).unwrap(),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            gpu_class: gpu,
        }
    }

    #[test]
    fn within_ceiling_is_ok() {
        let mut engine = MockCompat::new();
        engine
            .expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.6").unwrap()));
        engine.expect_check_toolkit().returning(|_, _, _| Verdict {
            ok: true,
            severity: Severity::Ok,
            reason: "within ceiling".into(),
            forward_compat_possible: false,
        });
        let d = linux_driver("565.57.01", true, GpuClass::GeForce);
        let active = Version::parse("12.4.1").unwrap();
        let f = check_driver_ceiling(&engine, &d, Some(&active));
        assert_eq!(f.code, "DRIVER_CEILING");
        assert_eq!(f.severity, Severity::Ok);
    }

    #[test]
    fn exceeds_ceiling_warns_with_compat_hint_on_eligible_gpu() {
        let mut engine = MockCompat::new();
        engine
            .expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.4").unwrap()));
        engine.expect_check_toolkit().returning(|_, _, _| Verdict {
            ok: false,
            severity: Severity::Warn,
            reason: "toolkit exceeds driver ceiling".into(),
            forward_compat_possible: true,
        });
        let d = linux_driver("550.54.14", true, GpuClass::DataCenter);
        let active = Version::parse("12.9.0").unwrap();
        let f = check_driver_ceiling(&engine, &d, Some(&active));
        assert_eq!(f.severity, Severity::Warn);
        assert!(f.detail.contains("12.9.0"));
        assert!(f.detail.contains("12.4"));
        assert!(f.hint.as_deref().unwrap().contains("cuda-compat"));
    }

    #[test]
    fn no_driver_warns_build_only_ok_never_blocks() {
        let engine = MockCompat::new(); // engine never consulted when driver absent
        let d = linux_driver("0", false, GpuClass::Unknown);
        let active = Version::parse("12.4.1").unwrap();
        let f = check_driver_ceiling(&engine, &d, Some(&active));
        assert_eq!(f.code, "DRIVER_ABSENT");
        assert_eq!(f.severity, Severity::Warn);
        assert!(f.detail.to_lowercase().contains("build-only"));
    }

    #[test]
    fn no_active_toolkit_reports_driver_ceiling_only() {
        let mut engine = MockCompat::new();
        engine
            .expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.6").unwrap()));
        let d = linux_driver("565.57.01", true, GpuClass::GeForce);
        let f = check_driver_ceiling(&engine, &d, None);
        assert_eq!(f.code, "DRIVER_CEILING");
        assert_eq!(f.severity, Severity::Ok);
        assert!(f.detail.contains("12.6"));
    }
}

#[cfg(test)]
mod hygiene_tests {
    use super::*;
    use crate::Severity;

    fn snap(path: &str, ld: &str, home: &str, root: &str) -> EnvSnapshot {
        EnvSnapshot {
            path: path.into(),
            ld_library_path: ld.into(),
            cuda_home: Some(home.into()),
            active_root: Some(root.into()),
            path_sep: ':',
        }
    }

    #[test]
    fn clean_single_cuda_on_path_is_ok() {
        let s = snap(
            "/home/u/.cuvm/versions/12.4.1/bin:/usr/bin",
            "/home/u/.cuvm/versions/12.4.1/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        assert!(fs.iter().all(|f| f.severity == Severity::Ok), "{fs:#?}");
    }

    #[test]
    fn dup_cuda_dirs_warn() {
        let s = snap(
            "/opt/cuda-12.2/bin:/home/u/.cuvm/versions/12.4.1/bin:/usr/bin",
            "/opt/cuda-12.2/lib64:/home/u/.cuvm/versions/12.4.1/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        let dup = fs
            .iter()
            .find(|f| f.code == "PATH_DUP_CUDA")
            .expect("dup finding");
        assert_eq!(dup.severity, Severity::Warn);
        assert!(dup.detail.contains("/opt/cuda-12.2/bin"));
        assert!(dup.detail.contains("12.4.1/bin"));
    }

    #[test]
    fn nvcc_resolving_outside_cuda_home_blocks() {
        // stale 12.2 bin comes FIRST on PATH, so nvcc resolves there, but CUDA_HOME=12.4.1
        let s = snap(
            "/opt/cuda-12.2/bin:/home/u/.cuvm/versions/12.4.1/bin:/usr/bin",
            "/opt/cuda-12.2/lib64:/home/u/.cuvm/versions/12.4.1/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        let mm = fs
            .iter()
            .find(|f| f.code == "NVCC_MISMATCH")
            .expect("mismatch finding");
        assert_eq!(mm.severity, Severity::Block);
        assert!(mm.detail.contains("/opt/cuda-12.2/bin"));
        assert!(mm.detail.contains("/home/u/.cuvm/versions/12.4.1"));
        assert!(mm.hint.as_deref().unwrap().contains("cuvm use"));
    }

    #[test]
    fn stale_cuda_bin_not_matching_active_root_warns() {
        let s = snap(
            "/opt/cuda-11.8/bin:/usr/bin",
            "/opt/cuda-11.8/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        let stale = fs
            .iter()
            .find(|f| f.code == "PATH_STALE_CUDA")
            .expect("stale finding");
        assert_eq!(stale.severity, Severity::Warn);
        assert!(stale.detail.contains("/opt/cuda-11.8/bin"));
    }

    #[test]
    fn no_cuda_home_yields_no_mismatch_only_info() {
        let s = EnvSnapshot {
            path: "/usr/bin".into(),
            ld_library_path: String::new(),
            cuda_home: None,
            active_root: None,
            path_sep: ':',
        };
        let fs = check_path_hygiene(&s);
        assert!(fs.iter().all(|f| f.severity == Severity::Ok), "{fs:#?}");
    }
}

#[cfg(test)]
mod aggregate_tests {
    use super::*;
    use crate::{CompatEngine, Severity, Verdict};
    use cuvm_core::{Arch, Driver, GpuClass, Os, Platform, Version};
    use mockall::mock;

    mock! {
        pub Eng {}
        impl CompatEngine for Eng {
            fn max_toolkit_for_driver(&self, d: &Driver) -> anyhow::Result<Version>;
            fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict;
            fn pair_cudnn(&self, toolkit: &Version, available: &[Version]) -> Option<Version>;
            fn validate_pair(&self, toolkit: &Version, cudnn: &Version) -> Verdict;
        }
    }

    fn broken_snapshot() -> EnvSnapshot {
        EnvSnapshot {
            path: "/opt/cuda-12.2/bin:/home/u/.cuvm/versions/12.4.1/bin:/usr/bin".into(),
            ld_library_path: "/opt/cuda-12.2/lib64:/home/u/.cuvm/versions/12.4.1/lib64".into(),
            cuda_home: Some("/home/u/.cuvm/versions/12.4.1".into()),
            active_root: Some("/home/u/.cuvm/versions/12.4.1".into()),
            path_sep: ':',
        }
    }

    #[test]
    fn broken_path_dup_and_nvcc_mismatch_snapshot() {
        let mut engine = MockEng::new();
        engine
            .expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.6").unwrap()));
        engine.expect_check_toolkit().returning(|_, _, _| Verdict {
            ok: true,
            severity: Severity::Ok,
            reason: "ok".into(),
            forward_compat_possible: false,
        });
        let driver = Driver {
            present: true,
            version: Version::parse("565.57.01").unwrap(),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            gpu_class: GpuClass::GeForce,
        };
        let active = Version::parse("12.4.1").unwrap();
        let report = run_doctor(&engine, &driver, Some(&active), &broken_snapshot());

        // The NVCC_MISMATCH block drives a nonzero exit.
        assert_eq!(report.exit_code(), 2);
        insta::assert_snapshot!(report.to_string());
    }
}
