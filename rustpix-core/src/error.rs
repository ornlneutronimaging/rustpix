//! Error types for rustpix.

use thiserror::Error;

/// Errors during clustering operations.
#[derive(Error, Debug)]
pub enum ClusteringError {
    /// No hits provided for clustering.
    #[error("empty input: cannot cluster zero hits")]
    EmptyInput,

    /// Invalid clustering configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Internal state error while clustering.
    #[error("state error: {0}")]
    StateError(String),
}

/// Errors during extraction operations.
#[derive(Error, Debug)]
pub enum ExtractionError {
    /// Cluster is empty when extraction is attempted.
    #[error("empty cluster: cannot extract from zero hits")]
    EmptyCluster,

    /// Invalid extraction configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Errors during I/O operations.
#[derive(Error, Debug)]
pub enum IoError {
    /// File path does not exist.
    #[error("file not found: {0}")]
    FileNotFound(String),

    /// File data did not match expected format.
    #[error("invalid file format: {0}")]
    InvalidFormat(String),

    /// Memory-mapped file failed to initialize.
    #[error("memory mapping failed: {0}")]
    MmapError(String),

    /// Underlying I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors during TPX3 processing.
#[derive(Error, Debug)]
pub enum ProcessingError {
    /// Underlying I/O error during processing.
    #[error("I/O error: {0}")]
    Io(#[from] IoError),

    /// Packet failed validation.
    #[error("invalid packet at offset {offset}: {message}")]
    InvalidPacket {
        /// Byte offset where the invalid packet was detected.
        offset: usize,
        /// Validation error description.
        message: String,
    },

    /// TDC reference missing for a data section.
    #[error("missing TDC reference for section starting at offset {0}")]
    MissingTdc(usize),

    /// Invalid or inconsistent processing configuration.
    #[error("configuration error: {0}")]
    Config(String),
}

/// Combined error type for the library.
#[derive(Error, Debug)]
pub enum Error {
    /// Error from clustering stage.
    #[error("clustering error: {0}")]
    Clustering(#[from] ClusteringError),

    /// Error from extraction stage.
    #[error("extraction error: {0}")]
    Extraction(#[from] ExtractionError),

    /// Error from I/O operations.
    #[error("I/O error: {0}")]
    Io(#[from] IoError),

    /// Error from TPX3 processing pipeline.
    #[error("processing error: {0}")]
    Processing(#[from] ProcessingError),
}

/// Result type alias using the combined Error.
pub type Result<T> = std::result::Result<T, Error>;
