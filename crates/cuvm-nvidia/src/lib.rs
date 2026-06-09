//! cuvm-nvidia — nvidia-smi driver probe (graceful-absent).

#![forbid(unsafe_code)]

pub mod smi;

pub use smi::SmiProbe;

/// Factory: create a boxed `DriverProbe` backed by `nvidia-smi`.
#[must_use]
pub fn new_driver_probe() -> Box<dyn cuvm_app::DriverProbe> {
    Box::new(SmiProbe::new())
}
