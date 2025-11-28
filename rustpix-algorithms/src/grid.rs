//! Grid-based clustering algorithm.
//!
//! Uses spatial indexing to efficiently cluster hits by dividing
//! the detector into grid cells.
//! See IMPLEMENTATION_PLAN.md Part 4.4 for detailed specification.

use crate::SpatialGrid;
use rustpix_core::clustering::{
    ClusteringConfig, ClusteringError, ClusteringState, ClusteringStatistics, HitClustering,
};
use rustpix_core::hit::Hit;

/// Grid clustering configuration.
#[derive(Clone, Debug)]
pub struct GridConfig {
    /// Cell size for the grid (pixels).
    pub cell_size: usize,
    /// Spatial radius for neighbor queries (pixels).
    pub radius: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
    /// Whether to use parallel processing.
    pub parallel: bool,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            cell_size: 32,
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            parallel: true,
        }
    }
}

/// Grid clustering state.
pub struct GridState {
    hits_processed: usize,
    clusters_found: usize,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            hits_processed: 0,
            clusters_found: 0,
        }
    }
}

impl ClusteringState for GridState {
    fn reset(&mut self) {
        self.hits_processed = 0;
        self.clusters_found = 0;
    }
}

/// Grid-based clustering with spatial indexing.
///
/// TODO: Full implementation in IMPLEMENTATION_PLAN.md Part 4.4
pub struct GridClustering {
    config: GridConfig,
    generic_config: ClusteringConfig,
}

impl GridClustering {
    /// Create with custom configuration.
    pub fn new(config: GridConfig) -> Self {
        let generic_config = ClusteringConfig {
            radius: config.radius,
            temporal_window_ns: config.temporal_window_ns,
            min_cluster_size: config.min_cluster_size,
            max_cluster_size: None,
        };
        Self {
            config,
            generic_config,
        }
    }

    /// Set whether to use parallel processing.
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.config.parallel = parallel;
        self
    }

    /// Get cell size.
    pub fn cell_size(&self) -> usize {
        self.config.cell_size
    }
}

impl Default for GridClustering {
    fn default() -> Self {
        Self::new(GridConfig::default())
    }
}

impl HitClustering for GridClustering {
    type State = GridState;

    fn name(&self) -> &'static str {
        "Grid"
    }

    fn create_state(&self) -> Self::State {
        GridState::default()
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.radius = config.radius;
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
        // TODO: Implement grid clustering algorithm
        // See IMPLEMENTATION_PLAN.md Part 4.4 for full specification
        //
        // Algorithm outline:
        // 1. Build spatial grid index
        // 2. For each hit, query 3x3 neighborhood
        // 3. Use union-find to merge spatially/temporally close hits
        // 4. Assign cluster labels

        if hits.is_empty() {
            return Ok(0);
        }

        // Build spatial index
        let mut grid: SpatialGrid<usize> =
            SpatialGrid::new(self.config.cell_size, 512, 512);

        for (i, hit) in hits.iter().enumerate() {
            grid.insert(hit.x() as i32, hit.y() as i32, i);
        }

        // Placeholder: assign each hit to its own cluster
        // TODO: Implement proper union-find based clustering
        for (i, label) in labels.iter_mut().enumerate() {
            *label = i as i32;
        }

        state.hits_processed = hits.len();
        state.clusters_found = hits.len();

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_config_defaults() {
        let config = GridConfig::default();
        assert_eq!(config.cell_size, 32);
        assert_eq!(config.radius, 5.0);
        assert!(config.parallel);
    }

    #[test]
    fn test_grid_state_reset() {
        let mut state = GridState::default();
        state.hits_processed = 100;
        state.clusters_found = 10;
        state.reset();
        assert_eq!(state.hits_processed, 0);
        assert_eq!(state.clusters_found, 0);
    }

    #[test]
    fn test_grid_with_parallel() {
        let algo = GridClustering::default().with_parallel(false);
        assert!(!algo.config.parallel);
    }
}
