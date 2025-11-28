//! I/O error types.

use thiserror::Error;

/// Result type for I/O operations.
pub type Result<T> = std::result::Result<T, Error>;

/// I/O error types.
#[derive(Error, Debug)]
pub enum Error {
    /// File I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Memory mapping error.
    #[error("memory mapping error: {0}")]
    MmapError(String),

    /// Invalid file format.
    #[error("invalid file format: {0}")]
    InvalidFormat(String),

    /// Core library error.
    #[error("core error: {0}")]
    CoreError(#[from] rustpix_core::Error),
}
