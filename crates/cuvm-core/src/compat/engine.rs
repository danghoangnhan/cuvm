//! Pure compatibility engine over the embedded §12 tables.
//!
//! Lives in `cuvm-core` to keep the algorithm I/O-free and unit-testable. The
//! `cuvm-app::CompatEngine` trait is implemented as a thin adapter in WU-8
//! (mapping [`CompatOutcome`] -> `app::Verdict`). All version comparisons use
//! `Version`'s numeric tuple `Ord` (spec §2.4: never lexical).

use crate::compat::tables::{CudnnMatrix, DriverCeilingTable};
use crate::domain::Os;
use crate::version::Version;
use crate::{Driver, GpuClass};

/// Core-side severity (mirrors `app::Severity`; kept separate so `cuvm-core`
/// owns no `cuvm-app` dependency — the Dependency Rule, spec §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatSeverity {
    Ok,
    Warn,
    Block,
}

/// Core-side verdict. WU-8 maps this onto `app::Verdict`.
#[derive(Debug, Clone)]
pub struct CompatOutcome {
    pub ok: bool,
    pub severity: CompatSeverity,
    pub reason: String,
    pub forward_compat_possible: bool,
}

/// Minor-version-compatibility floors ("likely works") from spec §12/§2.4.
const FLOOR_12X: &str = "525.60.13";
const FLOOR_13X: &str = "580.65.06";

/// Error for `max_toolkit_for_driver` when no row matches.
#[derive(Debug, thiserror::Error)]
pub enum CompatLookupError {
    #[error("no CUDA toolkit ceiling for this driver/OS")]
    NoCeiling,
}

/// Default `CompatEngine` over the embedded tables.
pub struct DefaultCompatEngine {
    drivers: DriverCeilingTable,
    cudnn: CudnnMatrix,
}

impl Default for DefaultCompatEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultCompatEngine {
    #[must_use]
    pub fn new() -> Self {
        DefaultCompatEngine {
            drivers: DriverCeilingTable::load(),
            cudnn: CudnnMatrix::load(),
        }
    }

    /// Per-OS minimum driver for a CUDA release, or `None` if that OS is N/A
    /// (e.g. all of CUDA 13.x on Windows).
    fn os_min(row: &crate::compat::tables::DriverRow, os: Os) -> Option<&Version> {
        match os {
            Os::Linux => Some(&row.linux_min),
            Os::Windows => row.windows_min.as_ref(),
        }
    }

    /// The minor-version-compat floor for a toolkit's CUDA major.
    fn floor_for(toolkit: &Version) -> Version {
        let s = if toolkit.major() >= 13 {
            FLOOR_13X
        } else {
            FLOOR_12X
        };
        Version::parse(s).expect("embedded floor constant parses")
    }

    /// `cuda-compat` is Linux-only and never applies to `GeForce` (spec §2.4).
    fn forward_compat_eligible(d: &Driver) -> bool {
        d.platform.os == Os::Linux
            && matches!(
                d.gpu_class,
                GpuClass::DataCenter | GpuClass::NgcReadyRtx | GpuClass::Jetson
            )
    }

    /// Inverse lookup: highest CUDA whose per-OS minimum ≤ driver.
    ///
    /// # Errors
    /// Returns [`CompatLookupError::NoCeiling`] if no row satisfies the condition.
    pub fn max_toolkit_for_driver(&self, d: &Driver) -> Result<Version, CompatLookupError> {
        let os = d.platform.os;
        let mut best: Option<&Version> = None;
        for row in &self.drivers.rows {
            if let Some(min) = Self::os_min(row, os) {
                if *min <= d.version {
                    match best {
                        Some(b) if row.cuda <= *b => {}
                        _ => best = Some(&row.cuda),
                    }
                }
            }
        }
        best.cloned().ok_or(CompatLookupError::NoCeiling)
    }

    /// Strict (exact per-release minimum) or likely (minor-version floor) check.
    #[must_use]
    pub fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> CompatOutcome {
        let fwd = Self::forward_compat_eligible(d);
        let os = d.platform.os;

        // Toolkits are patch-versioned (e.g. 12.4.1); the table is keyed by line.
        let want_line = want.major_minor();
        let Some(row) = self.drivers.row_for(&want_line) else {
            return CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!("unknown CUDA toolkit {} (not in compat table)", want.raw),
                forward_compat_possible: fwd,
            };
        };

        let Some(strict_min) = Self::os_min(row, os) else {
            return CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!("CUDA {} is N/A on this OS", want.raw),
                forward_compat_possible: fwd,
            };
        };

        if d.version >= *strict_min {
            return CompatOutcome {
                ok: true,
                severity: CompatSeverity::Ok,
                reason: format!(
                    "driver {} satisfies CUDA {} minimum {}",
                    d.version.raw, want.raw, strict_min.raw
                ),
                forward_compat_possible: fwd,
            };
        }

        if strict {
            return CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!(
                    "driver {} is below CUDA {} minimum {} (use --force or cuda-compat)",
                    d.version.raw, want.raw, strict_min.raw
                ),
                forward_compat_possible: fwd,
            };
        }

        // Non-strict: above the minor-version floor -> likely works (Warn).
        let floor = Self::floor_for(want);
        if d.version >= floor {
            CompatOutcome {
                ok: false,
                severity: CompatSeverity::Warn,
                reason: format!(
                    "driver {} below strict minimum {} but above the {}.x minor-version floor {} (likely works)",
                    d.version.raw, strict_min.raw, want.major(), floor.raw
                ),
                forward_compat_possible: fwd,
            }
        } else {
            CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!(
                    "driver {} below the {}.x minor-version floor {}",
                    d.version.raw,
                    want.major(),
                    floor.raw
                ),
                forward_compat_possible: fwd,
            }
        }
    }

    /// Newest available cuDNN whose line supports the toolkit's CUDA major.
    #[must_use]
    pub fn pair_cudnn(&self, toolkit: &Version, available: &[Version]) -> Option<Version> {
        let major = toolkit.major();
        available
            .iter()
            .filter(|cand| {
                self.cudnn
                    .entry_for(cand)
                    .is_some_and(|e| e.cuda_majors.contains(&major))
            })
            .max()
            .cloned()
    }

    /// Validate an explicit toolkit/cuDNN pairing by CUDA major.
    #[must_use]
    pub fn validate_pair(&self, toolkit: &Version, cudnn: &Version) -> CompatOutcome {
        let major = toolkit.major();
        let supported = self
            .cudnn
            .entry_for(cudnn)
            .is_some_and(|e| e.cuda_majors.contains(&major));
        if supported {
            CompatOutcome {
                ok: true,
                severity: CompatSeverity::Ok,
                reason: format!("cuDNN {} supports CUDA {}.x", cudnn.raw, major),
                forward_compat_possible: false,
            }
        } else {
            let need = if major >= 13 {
                "9.x"
            } else if major <= 11 {
                "8.x"
            } else {
                "8.x/9.x"
            };
            CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!(
                    "cuDNN {} does not support CUDA {}.x (needs cuDNN {})",
                    cudnn.raw, major, need
                ),
                forward_compat_possible: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Arch, Os, Platform};
    use crate::version::Version;
    use crate::{Driver, GpuClass};

    fn linux_driver(ver: &str, class: GpuClass) -> Driver {
        Driver {
            present: true,
            version: Version::parse(ver).unwrap(),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            gpu_class: class,
        }
    }

    fn windows_driver(ver: &str) -> Driver {
        Driver {
            present: true,
            version: Version::parse(ver).unwrap(),
            platform: Platform {
                os: Os::Windows,
                arch: Arch::X86_64,
            },
            gpu_class: GpuClass::GeForce,
        }
    }

    #[test]
    fn ceiling_linux_550_54_14_is_12_4() {
        let e = DefaultCompatEngine::new();
        let d = linux_driver("550.54.14", GpuClass::GeForce);
        assert_eq!(
            e.max_toolkit_for_driver(&d).unwrap(),
            Version::parse("12.4").unwrap()
        );
    }

    #[test]
    fn ceiling_linux_565_is_12_6() {
        let e = DefaultCompatEngine::new();
        // 12.6 min = 560.28.03 (<=565), 12.8 min = 570.26 (>565) -> ceiling 12.6.
        let d = linux_driver("565.57.01", GpuClass::GeForce);
        assert_eq!(
            e.max_toolkit_for_driver(&d).unwrap(),
            Version::parse("12.6").unwrap()
        );
    }

    #[test]
    fn ceiling_linux_552_is_12_4() {
        let e = DefaultCompatEngine::new();
        // 12.4 min = 550.54.14 (<=552), 12.5 min = 555.42.02 (>552) -> ceiling 12.4.
        let d = linux_driver("552.12", GpuClass::GeForce);
        assert_eq!(
            e.max_toolkit_for_driver(&d).unwrap(),
            Version::parse("12.4").unwrap()
        );
    }

    #[test]
    fn ceiling_uses_numeric_tuple_compare_not_lexical() {
        let e = DefaultCompatEngine::new();
        // 570.26 < 570.124.06 numerically (lexical would say "570.124.06" < "570.26").
        // Driver 570.124.06 must clear 12.8's 570.26 minimum -> ceiling >= 12.8.
        let d = linux_driver("570.124.06", GpuClass::DataCenter);
        let ceiling = e.max_toolkit_for_driver(&d).unwrap();
        assert!(
            ceiling >= Version::parse("12.8").unwrap(),
            "got {ceiling:?}"
        );
        // 12.9 min = 575.51.03 (>570.124.06) so ceiling stays 12.8.
        assert_eq!(ceiling, Version::parse("12.8").unwrap());
    }

    #[test]
    fn ceiling_windows_skips_na_13x_rows() {
        let e = DefaultCompatEngine::new();
        // A high Windows driver still cannot reach any 13.x (all N/A) -> caps at 12.9.
        let d = windows_driver("999.99");
        assert_eq!(
            e.max_toolkit_for_driver(&d).unwrap(),
            Version::parse("12.9").unwrap()
        );
    }

    #[test]
    fn check_toolkit_strict_blocks_below_exact_minimum() {
        let e = DefaultCompatEngine::new();
        // 12.4 strict min = 550.54.14; driver 545.x is below -> Block.
        let d = linux_driver("545.23.06", GpuClass::GeForce);
        let out = e.check_toolkit(&d, &Version::parse("12.4").unwrap(), true);
        assert_eq!(out.severity, CompatSeverity::Block);
        assert!(!out.ok);
    }

    #[test]
    fn check_toolkit_strict_ok_at_or_above_minimum() {
        let e = DefaultCompatEngine::new();
        let d = linux_driver("550.54.14", GpuClass::GeForce);
        let out = e.check_toolkit(&d, &Version::parse("12.4").unwrap(), true);
        assert_eq!(out.severity, CompatSeverity::Ok);
        assert!(out.ok);
    }

    #[test]
    fn check_toolkit_accepts_patch_version_via_minor_row() {
        let e = DefaultCompatEngine::new();
        // Real installed toolkits carry a patch (12.4.1); it must resolve to the
        // 12.4 line row, not fall through to "unknown toolkit". Driver 565 clears
        // the 12.4 minimum (550.54.14), so strict -> Ok.
        let d = linux_driver("565.57.01", GpuClass::GeForce);
        let out = e.check_toolkit(&d, &Version::parse("12.4.1").unwrap(), true);
        assert_eq!(out.severity, CompatSeverity::Ok);
        assert!(out.ok);
    }

    #[test]
    fn check_toolkit_likely_warns_above_floor_below_strict() {
        let e = DefaultCompatEngine::new();
        // Non-strict: 12.x floor = 525.60.13. Driver 540 >= floor but < 12.4 strict
        // (550.54.14) -> Warn (minor-version-compat likely-works), not Block.
        let d = linux_driver("540.00.00", GpuClass::GeForce);
        let out = e.check_toolkit(&d, &Version::parse("12.4").unwrap(), false);
        assert_eq!(out.severity, CompatSeverity::Warn);
        assert!(!out.ok);
    }

    #[test]
    fn check_toolkit_likely_blocks_below_floor() {
        let e = DefaultCompatEngine::new();
        // 13.x floor = 580.65.06; driver 560 is below the floor -> Block even non-strict.
        let d = linux_driver("560.28.03", GpuClass::DataCenter);
        let out = e.check_toolkit(&d, &Version::parse("13.0").unwrap(), false);
        assert_eq!(out.severity, CompatSeverity::Block);
    }

    #[test]
    fn check_toolkit_forward_compat_flag_only_for_eligible_gpu_on_linux() {
        let e = DefaultCompatEngine::new();
        // Below strict, DataCenter Linux -> forward_compat_possible = true.
        let dc = linux_driver("545.23.06", GpuClass::DataCenter);
        let out_dc = e.check_toolkit(&dc, &Version::parse("12.4").unwrap(), true);
        assert!(out_dc.forward_compat_possible);
        // GeForce never qualifies for cuda-compat.
        let gf = linux_driver("545.23.06", GpuClass::GeForce);
        let out_gf = e.check_toolkit(&gf, &Version::parse("12.4").unwrap(), true);
        assert!(!out_gf.forward_compat_possible);
        // Windows never qualifies (Linux only).
        let win = windows_driver("520.06");
        let out_win = e.check_toolkit(&win, &Version::parse("12.4").unwrap(), true);
        assert!(!out_win.forward_compat_possible);
    }

    #[test]
    fn pair_cudnn_picks_newest_supporting_line() {
        let e = DefaultCompatEngine::new();
        let avail = vec![
            Version::parse("8.9.7").unwrap(),
            Version::parse("9.23.0").unwrap(),
        ];
        // CUDA 13 -> only 9.x supports it.
        assert_eq!(
            e.pair_cudnn(&Version::parse("13.0").unwrap(), &avail),
            Some(Version::parse("9.23.0").unwrap())
        );
        // CUDA 11 -> only 8.x.
        assert_eq!(
            e.pair_cudnn(&Version::parse("11.8").unwrap(), &avail),
            Some(Version::parse("8.9.7").unwrap())
        );
        // CUDA 12 -> both support; pick newest (9.23.0).
        assert_eq!(
            e.pair_cudnn(&Version::parse("12.4").unwrap(), &avail),
            Some(Version::parse("9.23.0").unwrap())
        );
    }

    #[test]
    fn pair_cudnn_none_when_nothing_supports() {
        let e = DefaultCompatEngine::new();
        let avail = vec![Version::parse("8.9.7").unwrap()];
        // 8.9.7 supports [11,12]; CUDA 13 is unsupported -> None.
        assert_eq!(e.pair_cudnn(&Version::parse("13.0").unwrap(), &avail), None);
    }

    #[test]
    fn validate_pair_blocks_13x_with_8x_cudnn() {
        let e = DefaultCompatEngine::new();
        // CUDA 13.x requires cuDNN 9.x; pairing with 8.9.7 must Block.
        let out = e.validate_pair(
            &Version::parse("13.0").unwrap(),
            &Version::parse("8.9.7").unwrap(),
        );
        assert_eq!(out.severity, CompatSeverity::Block);
        assert!(!out.ok);
    }

    #[test]
    fn validate_pair_blocks_11x_with_9x_cudnn() {
        let e = DefaultCompatEngine::new();
        // CUDA 11.x requires cuDNN 8.x; 9.23.0 supports [12,13] only -> Block.
        let out = e.validate_pair(
            &Version::parse("11.8").unwrap(),
            &Version::parse("9.23.0").unwrap(),
        );
        assert_eq!(out.severity, CompatSeverity::Block);
    }

    #[test]
    fn validate_pair_ok_for_supported_major() {
        let e = DefaultCompatEngine::new();
        let out = e.validate_pair(
            &Version::parse("13.3").unwrap(),
            &Version::parse("9.23.0").unwrap(),
        );
        assert_eq!(out.severity, CompatSeverity::Ok);
        assert!(out.ok);
    }
}
