//! rustpix-algorithms: Clustering algorithms for hit detection.
//!
//! This crate provides various clustering algorithms:
//! - **ABS** (Age-Based Spatial) - O(n) average, bucket-based primary
//! - **DBSCAN** - Density-based with noise handling
//! - **Graph** - Union-Find connected components
//! - **Grid** - Detector geometry optimized
//!

mod abs;
mod dbscan;
mod grid;
mod processing;
pub mod spatial;

pub use abs::{AbsClustering, AbsConfig, AbsState};
pub use dbscan::{DbscanClustering, DbscanConfig, DbscanState};
pub use grid::{GridClustering, GridConfig, GridState};
pub use processing::{cluster_and_extract, AlgorithmParams, ClusteringAlgorithm};
pub use spatial::SpatialGrid;

// Re-export core clustering traits
pub use rustpix_core::clustering::{ClusteringConfig, ClusteringStatistics};
