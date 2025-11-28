//! rustpix-core: Core traits and types for pixel detector data processing.
//!
//! This crate provides the foundational abstractions for hit detection,
//! neutron processing, clustering, and centroid extraction.

mod clustering;
mod error;
mod extraction;
mod hit;
mod neutron;

pub use clustering::{Cluster, ClusteringAlgorithm, ClusteringConfig};
pub use error::{Error, Result};
pub use extraction::{Centroid, CentroidExtractor, ExtractionConfig, WeightedCentroidExtractor};
pub use hit::{Hit, HitData, PixelCoord, TimeOfArrival};
pub use neutron::{Neutron, NeutronData};
