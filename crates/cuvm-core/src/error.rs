use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid version string: {raw:?}")]
    InvalidVersion { raw: String },
}
