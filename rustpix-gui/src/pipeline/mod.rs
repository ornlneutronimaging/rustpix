//! Processing pipeline modules for file loading and clustering.

mod clustering;
mod loader;

pub use clustering::{run_clustering_worker, ClusteringWorkerConfig};
pub use loader::load_file_worker;

/// Algorithm type selection for clustering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgorithmType {
    /// Age-Based Spatial clustering (streaming).
    Abs,
    /// DBSCAN density-based clustering.
    Dbscan,
    /// Grid-based spatial partitioning (fastest).
    Grid,
}

impl std::fmt::Display for AlgorithmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlgorithmType::Abs => write!(f, "ABS (Age-Based Spatial)"),
            AlgorithmType::Dbscan => write!(f, "DBSCAN"),
            AlgorithmType::Grid => write!(f, "Grid (Spatial Partition)"),
        }
    }
}
