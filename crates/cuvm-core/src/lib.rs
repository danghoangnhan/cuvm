//! cuvm-core — pure domain types with ZERO I/O dependencies.

#![forbid(unsafe_code)]

pub mod domain;
pub mod error;
pub mod version;

pub use domain::{
    Alias, Arch, Bundle, Companion, Cudnn, Driver, GpuClass, Os, Pin, Platform, Shell, Source,
    Toolkit,
};
pub use error::CoreError;
pub use version::Version;
