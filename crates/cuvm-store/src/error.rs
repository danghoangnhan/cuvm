//! Typed errors for cuvm-store I/O. No panics on bad on-disk data.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("could not resolve CUVM_HOME: {0}")]
    HomeUnresolved(String),

    #[error("manifest at {path} is not valid JSON: {source}")]
    Corrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "manifest at {path} has schema_version {found}, but this cuvm understands at \
         most {supported}; upgrade cuvm"
    )]
    SchemaTooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no bundle registered with handle {0}")]
    UnknownHandle(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_too_new_message_names_both_versions() {
        let e = StoreError::SchemaTooNew {
            path: PathBuf::from("/x/manifest.json"),
            found: 99,
            supported: 1,
        };
        let msg = e.to_string();
        assert!(msg.contains("99"));
        assert!(msg.contains("upgrade cuvm"));
    }

    #[test]
    fn unknown_handle_message_names_handle() {
        let e = StoreError::UnknownHandle("13.9.9".to_string());
        assert!(e.to_string().contains("13.9.9"));
    }
}
