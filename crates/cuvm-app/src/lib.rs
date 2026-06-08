//! cuvm-app — use-cases and trait ports. Depends only on `cuvm-core`.

#![forbid(unsafe_code)]

pub mod ports;

pub use ports::{
    AcquirePlan, Activator, Artifact, ArtifactKind, Cached, Candidate, CompatEngine,
    ComponentPolicy, DriverProbe, Installer, Inventory, RegistryClient, ResolveVia, Resolved,
    Resolver, Severity, Verdict,
};
