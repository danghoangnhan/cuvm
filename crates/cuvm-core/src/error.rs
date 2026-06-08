use thiserror::Error;

use crate::Version;

/// Errors from pure-core parsing / construction.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid version string: {raw:?}")]
    InvalidVersion { raw: String },
}

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

/// Typed error enum for the resolver / version-grammar layer.
///
/// Designed to be `PartialEq` so unit tests can assert on specific variants.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreErr {
    #[error("invalid version string {0:?}: {1}")]
    BadVersion(String, &'static str),

    #[error("no installed toolkit matches {spec:?}; run `cuvm install {spec}` to install it")]
    NotInstalled { spec: String },

    #[error("alias cycle detected while expanding {0:?}")]
    AliasCycle(String),

    #[error("alias {0:?} is not defined")]
    UnknownAlias(String),
}

/// Convenience `Result` alias for the resolver layer.
pub type Result<T> = std::result::Result<T, CoreErr>;

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
