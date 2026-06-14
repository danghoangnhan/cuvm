//! cuvm-core — pure domain types with ZERO I/O dependencies.
//!
//! No http, no fs, no async in the public API: just numeric versions, domain
//! structs, serde manifest value types, the OS-neutral `EnvPlan`, and errors.

#![forbid(unsafe_code)]

pub mod candidate;
pub mod compat;
pub mod domain;
pub mod env_plan;
pub mod envplan;
pub mod error;
pub mod manifest;
pub mod process_env;
pub mod version;

pub use candidate::Candidate;
pub use compat::{CompatLookupError, CompatOutcome, CompatSeverity, DefaultCompatEngine};
pub use domain::{
    current_platform, Alias, Arch, Bundle, Companion, Cudnn, Driver, GpuClass, Os, Pin, Platform,
    Shell, Source, Toolkit,
};
pub use env_plan::plan_for;
pub use envplan::EnvPlan;
pub use error::{CompatError, CoreError, CoreResult};
pub use manifest::{
    BundleRecord, CudnnRecord, DriverRecord, Manifest, VersionMeta, SCHEMA_VERSION,
};
pub use process_env::{process_env, EnvVar};
pub use version::Version;
