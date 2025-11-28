//! Clustering traits and types.

use crate::{Hit, HitData, Result};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A cluster of hits representing a single detection event.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Cluster<H = HitData> {
    /// Hits belonging to this cluster.
    pub hits: Vec<H>,
}

impl<H> Cluster<H> {
    /// Creates an empty cluster.
    pub fn new() -> Self {
        Self { hits: Vec::new() }
    }

    /// Creates a cluster with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            hits: Vec::with_capacity(capacity),
        }
    }

    /// Adds a hit to the cluster.
    pub fn push(&mut self, hit: H) {
        self.hits.push(hit);
    }

    /// Returns the number of hits in the cluster.
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    /// Returns true if the cluster is empty.
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    /// Returns an iterator over the hits.
    pub fn iter(&self) -> impl Iterator<Item = &H> {
        self.hits.iter()
    }
}

impl<H> FromIterator<H> for Cluster<H> {
    fn from_iter<I: IntoIterator<Item = H>>(iter: I) -> Self {
        Self {
            hits: iter.into_iter().collect(),
        }
    }
}

/// Configuration for clustering algorithms.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ClusteringConfig {
    /// Maximum spatial distance (in pixels) for hits to be in the same cluster.
    pub spatial_epsilon: f64,
    /// Maximum temporal distance (in time units) for hits to be in the same cluster.
    pub temporal_epsilon: u64,
    /// Minimum number of hits to form a valid cluster.
    pub min_cluster_size: usize,
    /// Maximum number of hits in a cluster (for filtering large artifacts).
    pub max_cluster_size: Option<usize>,
}

impl Default for ClusteringConfig {
    fn default() -> Self {
        Self {
            spatial_epsilon: 1.5,   // ~1.5 pixels for 8-connectivity
            temporal_epsilon: 1000, // 1 microsecond typical
            min_cluster_size: 1,
            max_cluster_size: None,
        }
    }
}

impl ClusteringConfig {
    /// Creates a new clustering configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the spatial epsilon value.
    pub fn with_spatial_epsilon(mut self, epsilon: f64) -> Self {
        self.spatial_epsilon = epsilon;
        self
    }

    /// Sets the temporal epsilon value.
    pub fn with_temporal_epsilon(mut self, epsilon: u64) -> Self {
        self.temporal_epsilon = epsilon;
        self
    }

    /// Sets the minimum cluster size.
    pub fn with_min_cluster_size(mut self, size: usize) -> Self {
        self.min_cluster_size = size;
        self
    }

    /// Sets the maximum cluster size.
    pub fn with_max_cluster_size(mut self, size: usize) -> Self {
        self.max_cluster_size = Some(size);
        self
    }
}

/// Trait for clustering algorithms.
///
/// Clustering algorithms group hits that are spatially and temporally
/// close together into clusters representing single detection events.
pub trait ClusteringAlgorithm<H: Hit>: Send + Sync {
    /// Clusters the given hits into groups.
    fn cluster(&self, hits: &[H], config: &ClusteringConfig) -> Result<Vec<Cluster<H>>>
    where
        H: Clone;

    /// Returns the name of the algorithm.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_operations() {
        let mut cluster = Cluster::with_capacity(10);
        assert!(cluster.is_empty());

        cluster.push(HitData::new(0, 0, 100, 10));
        cluster.push(HitData::new(1, 0, 110, 15));
        cluster.push(HitData::new(0, 1, 105, 12));

        assert_eq!(cluster.len(), 3);
        assert!(!cluster.is_empty());
    }

    #[test]
    fn test_clustering_config() {
        let config = ClusteringConfig::new()
            .with_spatial_epsilon(2.0)
            .with_temporal_epsilon(500)
            .with_min_cluster_size(2)
            .with_max_cluster_size(100);

        assert!((config.spatial_epsilon - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.temporal_epsilon, 500);
        assert_eq!(config.min_cluster_size, 2);
        assert_eq!(config.max_cluster_size, Some(100));
    }
}
