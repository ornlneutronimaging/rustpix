//! rustpix-algorithms: Clustering algorithms for hit detection.
//!
//! This crate provides various clustering algorithms:
//! - **ABS** (Age-Based Spatial) - O(n) average, bucket-based primary
//! - **DBSCAN** - Density-based with noise handling
//! - **Graph** - Union-Find connected components
//! - **Grid** - Detector geometry optimized
//!
//! See IMPLEMENTATION_PLAN.md Part 4 for detailed algorithm specifications.

mod abs;
mod dbscan;
mod graph;
mod grid;
pub mod soa_abs;
pub mod soa_dbscan;
pub mod soa_grid;
pub mod spatial;

pub use abs::{AbsClustering, AbsConfig};
pub use dbscan::{DbscanClustering, DbscanConfig, DbscanState};
pub use graph::GraphClustering;
pub use grid::{GridClustering, GridConfig, GridState};
pub use soa_abs::{SoAAbsClustering, SoAAbsConfig, SoAAbsState};
pub use soa_dbscan::{SoADbscanClustering, SoADbscanConfig};
pub use soa_grid::SoAGridClustering;
pub use spatial::SpatialGrid;

// Re-export core clustering traits
pub use rustpix_core::clustering::{
    ClusteringConfig, ClusteringState, ClusteringStatistics, HitClustering,
};
