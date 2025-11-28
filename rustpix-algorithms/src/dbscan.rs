//! DBSCAN clustering algorithm.
//!
//! Density-Based Spatial Clustering of Applications with Noise.
//! See IMPLEMENTATION_PLAN.md Part 4.2 for detailed specification.

use rustpix_core::clustering::{
    ClusteringConfig, ClusteringError, ClusteringState, ClusteringStatistics, HitClustering,
};
use rustpix_core::hit::Hit;

/// DBSCAN-specific configuration.
#[derive(Clone, Debug)]
pub struct DbscanConfig {
    /// Spatial radius for neighborhood (pixels).
    pub epsilon: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
    /// Minimum points to form a core point.
    pub min_points: usize,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
}

impl Default for DbscanConfig {
    fn default() -> Self {
        Self {
            epsilon: 5.0,
            temporal_window_ns: 75.0,
            min_points: 2,
            min_cluster_size: 1,
        }
    }
}

/// DBSCAN clustering state.
pub struct DbscanState {
    hits_processed: usize,
    clusters_found: usize,
    noise_points: usize,
}

impl Default for DbscanState {
    fn default() -> Self {
        Self {
            hits_processed: 0,
            clusters_found: 0,
            noise_points: 0,
        }
    }
}

impl ClusteringState for DbscanState {
    fn reset(&mut self) {
        self.hits_processed = 0;
        self.clusters_found = 0;
        self.noise_points = 0;
    }
}

/// DBSCAN clustering algorithm with spatial indexing.
///
/// TODO: Full implementation in IMPLEMENTATION_PLAN.md Part 4.2
pub struct DbscanClustering {
    config: DbscanConfig,
    generic_config: ClusteringConfig,
}

impl DbscanClustering {
    /// Create with custom configuration.
    pub fn new(config: DbscanConfig) -> Self {
        let generic_config = ClusteringConfig {
            radius: config.epsilon,
            temporal_window_ns: config.temporal_window_ns,
            min_cluster_size: config.min_cluster_size,
            max_cluster_size: None,
        };
        Self {
            config,
            generic_config,
        }
    }

    /// Get min_points parameter.
    pub fn min_points(&self) -> usize {
        self.config.min_points
    }
}

impl Default for DbscanClustering {
    fn default() -> Self {
        Self::new(DbscanConfig::default())
    }
}

impl HitClustering for DbscanClustering {
    type State = DbscanState;

    fn name(&self) -> &'static str {
        "DBSCAN"
    }

    fn create_state(&self) -> Self::State {
        DbscanState::default()
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.epsilon = config.radius;
        self.config.temporal_window_ns = config.temporal_window_ns;
        self.generic_config = config.clone();
    }

    fn config(&self) -> &ClusteringConfig {
        &self.generic_config
    }

    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError> {
        // TODO: Implement DBSCAN algorithm
        // See IMPLEMENTATION_PLAN.md Part 4.2 for full specification
        //
        // Algorithm outline:
        // 1. Build spatial index for efficient neighbor queries
        // 2. For each unvisited point:
        //    a. Find neighbors within epsilon
        //    b. If |neighbors| >= min_points: expand cluster
        //    c. Else: mark as noise (label = -1)
        // 3. Assign cluster labels

        if hits.is_empty() {
            return Ok(0);
        }

        // Placeholder: assign each hit to its own cluster
        for (i, label) in labels.iter_mut().enumerate() {
            *label = i as i32;
        }

        state.hits_processed = hits.len();
        state.clusters_found = hits.len();
        state.noise_points = 0;

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            noise_hits: state.noise_points,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbscan_config_defaults() {
        let config = DbscanConfig::default();
        assert_eq!(config.epsilon, 5.0);
        assert_eq!(config.min_points, 2);
    }

    #[test]
    fn test_dbscan_state_reset() {
        let mut state = DbscanState::default();
        state.hits_processed = 100;
        state.clusters_found = 10;
        state.noise_points = 5;
        state.reset();
        assert_eq!(state.hits_processed, 0);
        assert_eq!(state.clusters_found, 0);
        assert_eq!(state.noise_points, 0);
    }
}
