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

    /// None of the recommended/requested components were present in the manifest.
    #[error("no usable components found for this toolkit (wanted: {wanted})")]
    NoComponents {
        /// A human-readable join of the requested component names.
        wanted: String,
    },

    /// An underlying HTTP fetch (via `cuvm_download::http_get`) failed.
    #[error("registry HTTP request failed: {0}")]
    Http(String),
}

/// `Result` alias for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;

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
}
