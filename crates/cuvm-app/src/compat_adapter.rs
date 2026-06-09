//! Adapter: wraps `cuvm_core::DefaultCompatEngine` to implement `cuvm_app::CompatEngine`.
//!
//! Maps `CompatOutcome`→`Verdict` and `CompatSeverity`→`Severity` (near 1:1).
//! CRITICAL: `Driver{present:false}` is handled upstream (in doctor/current) before
//! calling the engine; the engine itself does not branch on `present`.

use anyhow::Result;
use cuvm_core::{CompatSeverity, DefaultCompatEngine, Driver, Version};

use crate::{CompatEngine, Severity, Verdict};

/// Newtype adapter: `DefaultCompatEngine` → `cuvm_app::CompatEngine`.
pub struct CompatEngineAdapter {
    inner: DefaultCompatEngine,
}

impl Default for CompatEngineAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CompatEngineAdapter {
    /// Create a new adapter over a fresh `DefaultCompatEngine`.
    #[must_use]
    pub fn new() -> Self {
        CompatEngineAdapter {
            inner: DefaultCompatEngine::new(),
        }
    }
}

fn map_severity(s: CompatSeverity) -> Severity {
    match s {
        CompatSeverity::Ok => Severity::Ok,
        CompatSeverity::Warn => Severity::Warn,
        CompatSeverity::Block => Severity::Block,
    }
}

impl CompatEngine for CompatEngineAdapter {
    fn max_toolkit_for_driver(&self, d: &Driver) -> Result<Version> {
        self.inner
            .max_toolkit_for_driver(d)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict {
        let outcome = self.inner.check_toolkit(d, want, strict);
        Verdict {
            ok: outcome.ok,
            severity: map_severity(outcome.severity),
            reason: outcome.reason,
            forward_compat_possible: outcome.forward_compat_possible,
        }
    }

    fn pair_cudnn(&self, tk: &Version, avail: &[Version]) -> Option<Version> {
        self.inner.pair_cudnn(tk, avail)
    }

    fn validate_pair(&self, tk: &Version, cudnn: &Version) -> Verdict {
        let outcome = self.inner.validate_pair(tk, cudnn);
        Verdict {
            ok: outcome.ok,
            severity: map_severity(outcome.severity),
            reason: outcome.reason,
            forward_compat_possible: outcome.forward_compat_possible,
        }
    }
}

/// Factory: create a boxed `CompatEngine` backed by the embedded compat tables.
#[must_use]
pub fn new_compat_engine() -> Box<dyn CompatEngine> {
    Box::new(CompatEngineAdapter::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::{Arch, Driver, GpuClass, Os, Platform, Version};

    fn linux_driver(ver: &str) -> Driver {
        Driver {
            present: true,
            version: Version::parse(ver).unwrap(),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            gpu_class: GpuClass::GeForce,
        }
    }

    #[test]
    fn adapter_max_toolkit_maps_correctly() {
        let adapter = CompatEngineAdapter::new();
        let d = linux_driver("565.57.01");
        let ceiling = adapter.max_toolkit_for_driver(&d).unwrap();
        assert_eq!(ceiling, Version::parse("12.6").unwrap());
    }

    #[test]
    fn adapter_check_toolkit_maps_severity() {
        let adapter = CompatEngineAdapter::new();
        let d = linux_driver("565.57.01");
        let want = Version::parse("12.4.1").unwrap();
        let verdict = adapter.check_toolkit(&d, &want, true);
        assert!(verdict.ok);
        assert_eq!(verdict.severity, Severity::Ok);
    }

    #[test]
    fn adapter_block_maps_to_app_block() {
        let adapter = CompatEngineAdapter::new();
        // Below the minor-version floor -> Block even with non-strict.
        let d = Driver {
            present: true,
            version: Version::parse("520.00").unwrap(),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            gpu_class: GpuClass::GeForce,
        };
        let want = Version::parse("13.0").unwrap();
        let verdict = adapter.check_toolkit(&d, &want, false);
        assert_eq!(verdict.severity, Severity::Block);
        assert!(!verdict.ok);
    }
}
