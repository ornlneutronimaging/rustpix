//! Error types for rustpix-core.

use thiserror::Error;

/// Result type alias for rustpix operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Core error types for rustpix operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Invalid pixel coordinate.
    #[error("invalid pixel coordinate: ({x}, {y})")]
    InvalidCoordinate { x: u16, y: u16 },

    /// Invalid time-of-arrival value.
    #[error("invalid time of arrival: {0}")]
    InvalidTimeOfArrival(u64),

    /// Invalid time-over-threshold value.
    #[error("invalid time over threshold: {0}")]
    InvalidTimeOverThreshold(u16),

    /// Clustering error.
    #[error("clustering error: {0}")]
    ClusteringError(String),

    /// Extraction error.
    #[error("extraction error: {0}")]
    ExtractionError(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    ConfigError(String),

    /// Empty cluster error.
    #[error("cannot compute centroid of empty cluster")]
    EmptyCluster,
}
