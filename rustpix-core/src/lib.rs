//! rustpix-core: Core traits and types for pixel detector data processing.
//!
//! This crate provides the foundational abstractions for hit detection,
//! neutron processing, clustering, and centroid extraction.
//!
#![warn(missing_docs)]

pub mod clustering;
pub mod error;
pub mod extraction;
pub mod neutron;
pub mod soa;

pub use clustering::{ClusteringConfig, ClusteringStatistics};
pub use error::{ClusteringError, Error, ExtractionError, IoError, ProcessingError, Result};
pub use extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
pub use neutron::{ClusterSize, Neutron, NeutronBatch, NeutronStatistics};
