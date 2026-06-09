//! cuvm-app — use-cases and trait ports. Depends only on `cuvm-core`.

#![forbid(unsafe_code)]

pub mod compat_adapter;
pub mod doctor;
pub mod ports;
pub mod resolver;

pub use compat_adapter::new_compat_engine;
pub use ports::{
    AcquirePlan, Activator, Artifact, ArtifactKind, Cached, Candidate, CompatEngine,
    ComponentPolicy, DriverProbe, Installer, Inventory, RegistryClient, ResolveVia, Resolved,
    Resolver, Severity, Verdict,
};
pub use resolver::MemResolver;
