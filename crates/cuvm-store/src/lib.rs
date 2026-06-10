//! cuvm-store: atomic manifest/meta I/O + content-addressed cudnn store.

#![forbid(unsafe_code)]

pub mod atomic;
pub mod cudnn_store;
pub mod error;
pub mod inventory;
pub mod layout;
pub mod manifest_io;
pub mod meta_io;
pub mod redist_cache;

pub use atomic::write_atomic;
pub use error::{Result, StoreError};
pub use inventory::FsInventory;
pub use layout::Layout;
pub use manifest_io::{read_manifest, write_manifest};
pub use meta_io::{read_meta, write_meta};

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
