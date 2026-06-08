//! cuvm-platform — per-OS Activator/Installer backends behind a runtime factory.
//!
//! WU-1: stub backends returning `NotImplemented`. Real syscalls (registry,
//! junction, broadcast, symlink) arrive behind `#[cfg]` in WU-5/WU-9/WU-13/WU-14.

pub mod unix;
pub mod windows;

/// Stable "not implemented yet" error for WU-1 stubs.
pub(crate) fn not_impl(what: &str) -> anyhow::Error {
    anyhow::anyhow!("{what}: not implemented (WU-1 stub)")
}
