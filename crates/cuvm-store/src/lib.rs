//! cuvm-store: atomic manifest/meta I/O + content-addressed cudnn store.

#![forbid(unsafe_code)]

pub mod error;

pub use error::{Result, StoreError};
