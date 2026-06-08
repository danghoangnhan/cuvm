//! cuvm-store: atomic manifest/meta I/O + content-addressed cudnn store.

#![forbid(unsafe_code)]

pub mod error;
pub mod layout;

pub use error::{Result, StoreError};
pub use layout::Layout;

impl Layout {
    /// Resolve from the real process environment and OS home directory.
    ///
    /// # Errors
    /// Returns [`StoreError::HomeUnresolved`] if neither `CUVM_HOME` nor the
    /// OS home directory can be determined.
    pub fn resolve() -> crate::error::Result<Self> {
        Layout::resolve_with(|k| std::env::var(k).ok(), os_home_dir())
    }
}

fn os_home_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(std::path::PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}
