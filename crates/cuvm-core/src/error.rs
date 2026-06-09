use thiserror::Error;

use crate::Version;

/// Errors from pure-core parsing / construction and resolver logic.
///
/// Derives `PartialEq` / `Eq` so unit tests can `assert_eq!` on specific variants.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid version string: {raw:?}")]
    InvalidVersion { raw: String },

    #[error("no installed toolkit matches {spec:?}; run `cuvm install {spec}` to install it")]
    NotInstalled { spec: String },

    #[error("alias cycle detected while expanding {0:?}")]
    AliasCycle(String),

    #[error("alias {0:?} is not defined")]
    UnknownAlias(String),
}

/// Convenience `Result` alias for the resolver / core-logic layer.
///
/// Named `CoreResult` (not `Result`) to avoid shadowing `std::result::Result`
/// at crate-root scope.
pub type CoreResult<T> = std::result::Result<T, CoreError>;

/// Compatibility / resolution decisions surfaced by the compat engine and resolver.
#[derive(Debug, Error)]
pub enum CompatError {
    #[error("no toolkit matching {spec:?} is installed (not installed)")]
    NotInstalled { spec: String },

    #[error("requested CUDA {want} exceeds the driver ceiling {ceiling}")]
    DriverCeiling { want: Version, ceiling: Version },

    #[error("cuDNN major {cudnn_major} is incompatible with CUDA major {cuda_major}")]
    CudnnMismatch { cuda_major: u32, cudnn_major: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Version;

    #[test]
    fn core_error_invalid_version_displays_raw() {
        let e = CoreError::InvalidVersion { raw: "12.x".into() };
        assert!(e.to_string().contains("12.x"));
    }

    #[test]
    fn core_error_not_installed_offers_install() {
        let e = CoreError::NotInstalled {
            spec: "11.8".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("11.8"));
        assert!(msg.contains("cuvm install 11.8"));
    }

    #[test]
    fn core_error_alias_cycle_names_alias() {
        let e = CoreError::AliasCycle("loop".into());
        assert!(e.to_string().contains("loop"));
    }

    #[test]
    fn core_error_unknown_alias_names_alias() {
        let e = CoreError::UnknownAlias("missing".into());
        assert!(e.to_string().contains("missing"));
    }

    #[test]
    fn core_error_is_partial_eq() {
        assert_eq!(
            CoreError::NotInstalled {
                spec: "12.4".into()
            },
            CoreError::NotInstalled {
                spec: "12.4".into()
            }
        );
        assert_ne!(
            CoreError::NotInstalled {
                spec: "12.4".into()
            },
            CoreError::NotInstalled {
                spec: "11.8".into()
            }
        );
    }

    #[test]
    fn compat_error_not_installed_names_the_spec() {
        let e = CompatError::NotInstalled {
            spec: "12.4".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("12.4"));
        assert!(msg.to_lowercase().contains("not installed"));
    }

    #[test]
    fn compat_error_driver_ceiling_reports_both_versions() {
        let e = CompatError::DriverCeiling {
            want: Version::parse("13.0").unwrap(),
            ceiling: Version::parse("12.4").unwrap(),
        };
        let msg = e.to_string();
        assert!(msg.contains("13.0"));
        assert!(msg.contains("12.4"));
    }

    #[test]
    fn compat_error_cudnn_mismatch_reports_majors() {
        let e = CompatError::CudnnMismatch {
            cuda_major: 13,
            cudnn_major: 8,
        };
        let msg = e.to_string();
        assert!(msg.contains("13"));
        assert!(msg.contains('8'));
    }
}
