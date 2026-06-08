//! cuvm-nvidia — nvidia-smi driver probe (graceful-absent).

#![forbid(unsafe_code)]

pub mod smi;

pub use smi::SmiProbe;
