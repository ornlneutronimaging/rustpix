//! rustpix-core: Core traits and types for pixel detector data processing.
//!
//! This crate provides the foundational abstractions for hit detection,
//! neutron processing, clustering, and centroid extraction.
//!

pub mod clustering;
pub mod error;
pub mod extraction;
pub mod hit;
pub mod neutron;
pub mod soa;

pub use clustering::{ClusteringConfig, ClusteringStatistics};
pub use error::{ClusteringError, Error, ExtractionError, IoError, ProcessingError, Result};
pub use extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
pub use hit::{ClusterableHit, GenericHit, Hit};
pub use neutron::{ClusterSize, Neutron, NeutronStatistics};
