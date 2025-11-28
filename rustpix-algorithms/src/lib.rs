//! rustpix-algorithms: Clustering algorithms with spatial indexing.
//!
//! This crate provides various clustering algorithms for hit detection:
//! - ABS (Adjacency-Based Search)
//! - DBSCAN (Density-Based Spatial Clustering of Applications with Noise)
//! - Graph-based clustering
//! - Grid-based clustering with spatial indexing

mod abs;
mod dbscan;
mod graph;
mod grid;
mod spatial;

pub use abs::AbsClustering;
pub use dbscan::DbscanClustering;
pub use graph::GraphClustering;
pub use grid::GridClustering;
pub use spatial::SpatialIndex;
