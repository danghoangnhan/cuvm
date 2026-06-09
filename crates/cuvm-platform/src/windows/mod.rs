//! Windows backend: runtime-dispatched script emission (compiles + golden-tests
//! on every host) and a thin win32 syscall floor for persistence + junctions.
//!
//! `activator`/`installer` are pure and always compile; `persist`/`junction`
//! split their bodies with `#[cfg(windows)]` + a non-windows stub so the crate
//! builds on the gnu/linux host (spec §3 — emission is runtime-dispatched).

pub mod activator;
pub mod installer;
pub mod junction;
pub mod persist;

pub use activator::WindowsActivator;
pub use installer::WindowsInstaller;
