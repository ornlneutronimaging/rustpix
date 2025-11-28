//! TPX3-specific error types.

use thiserror::Error;

/// Result type for TPX3 operations.
pub type Result<T> = std::result::Result<T, Error>;

/// TPX3-specific error types.
#[derive(Error, Debug)]
pub enum Error {
    /// Invalid packet header.
    #[error("invalid packet header: {0:#018x}")]
    InvalidPacketHeader(u64),

    /// Invalid packet type.
    #[error("invalid packet type: {0:#x}")]
    InvalidPacketType(u8),

    /// Packet parsing error.
    #[error("packet parsing error: {0}")]
    ParseError(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Core library error.
    #[error("core error: {0}")]
    CoreError(#[from] rustpix_core::Error),
}
