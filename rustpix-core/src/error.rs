//! Error types for rustpix.

use thiserror::Error;

/// Errors during clustering operations.
#[derive(Error, Debug)]
pub enum ClusteringError {
    #[error("empty input: cannot cluster zero hits")]
    EmptyInput,

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("state error: {0}")]
    StateError(String),
}

/// Errors during extraction operations.
#[derive(Error, Debug)]
pub enum ExtractionError {
    #[error("empty cluster: cannot extract from zero hits")]
    EmptyCluster,

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Errors during I/O operations.
#[derive(Error, Debug)]
pub enum IoError {
    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("invalid file format: {0}")]
    InvalidFormat(String),

    #[error("memory mapping failed: {0}")]
    MmapError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors during TPX3 processing.
#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("I/O error: {0}")]
    Io(#[from] IoError),

    #[error("invalid packet at offset {offset}: {message}")]
    InvalidPacket { offset: usize, message: String },

    #[error("missing TDC reference for section starting at offset {0}")]
    MissingTdc(usize),

    #[error("configuration error: {0}")]
    Config(String),
}

/// Combined error type for the library.
#[derive(Error, Debug)]
pub enum Error {
    #[error("clustering error: {0}")]
    Clustering(#[from] ClusteringError),

    #[error("extraction error: {0}")]
    Extraction(#[from] ExtractionError),

    #[error("I/O error: {0}")]
    Io(#[from] IoError),

    #[error("processing error: {0}")]
    Processing(#[from] ProcessingError),
}

/// Result type alias using the combined Error.
pub type Result<T> = std::result::Result<T, Error>;
