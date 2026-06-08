//! cuvm-app — use-cases and trait ports. Depends only on `cuvm-core`.

#![forbid(unsafe_code)]

pub mod ports;

pub use ports::{
    AcquirePlan, Artifact, ArtifactKind, Cached, Candidate, ComponentPolicy, ResolveVia, Resolved,
    Severity, Verdict,
};
