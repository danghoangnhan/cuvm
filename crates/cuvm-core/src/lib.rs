//! cuvm-core — pure domain types with ZERO I/O dependencies.
//!
//! No http, no fs, no async in the public API: just numeric versions, domain
//! structs, serde manifest value types, the OS-neutral `EnvPlan`, and errors.

#![forbid(unsafe_code)]

pub mod candidate;
pub mod domain;
pub mod envplan;
pub mod error;
pub mod manifest;
pub mod version;

pub use candidate::Candidate;
pub use domain::{
    current_platform, Alias, Arch, Bundle, Companion, Cudnn, Driver, GpuClass, Os, Pin, Platform,
    Shell, Source, Toolkit,
};
pub use envplan::EnvPlan;
pub use error::{CompatError, CoreError, CoreResult};
pub use manifest::{BundleRecord, DriverRecord, Manifest, VersionMeta, SCHEMA_VERSION};
pub use version::Version;
