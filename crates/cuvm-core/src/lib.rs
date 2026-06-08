//! cuvm-core — pure domain types with ZERO I/O dependencies.

#![forbid(unsafe_code)]

pub mod error;
pub mod version;

pub use error::CoreError;
pub use version::Version;
