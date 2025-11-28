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

use crate::spatial::SpatialGrid;

/// Point classification in DBSCAN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointType {
    Undefined,
    Noise,
    Border,
    Core,
}

/// DBSCAN clustering state.
pub struct DbscanState {
    hits_processed: usize,
    clusters_found: usize,
    noise_points: usize,
    visited: Vec<bool>,
    point_types: Vec<PointType>,
    spatial_grid: SpatialGrid<usize>,
}

impl Default for DbscanState {
    fn default() -> Self {
        Self {
            hits_processed: 0,
            clusters_found: 0,
            noise_points: 0,
            visited: Vec::new(),
            point_types: Vec::new(),
            spatial_grid: SpatialGrid::new(32, 512, 512),
        }
    }
}

impl ClusteringState for DbscanState {
    fn reset(&mut self) {
        self.hits_processed = 0;
        self.clusters_found = 0;
        self.noise_points = 0;
        self.visited.clear();
        self.point_types.clear();
        self.spatial_grid.clear();
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

    fn find_neighbors<H: Hit>(
        &self,
        hits: &[H],
        point_idx: usize,
        epsilon_sq: f64,
        window_tof: u32,
        state: &DbscanState,
    ) -> Vec<usize> {
        let hit = &hits[point_idx];
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        let mut neighbors = Vec::new();

        for &neighbor_idx in state.spatial_grid.query_neighborhood(x, y) {
            if point_idx == neighbor_idx {
                continue;
            }
            let neighbor = &hits[neighbor_idx];
            if hit.within_temporal_window(neighbor, window_tof)
                && hit.distance_squared(neighbor) <= epsilon_sq
            {
                neighbors.push(neighbor_idx);
            }
        }
        neighbors
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_cluster<H: Hit>(
        &self,
        hits: &[H],
        _core_idx: usize,
        mut seeds: Vec<usize>,
        cluster_id: i32,
        epsilon_sq: f64,
        window_tof: u32,
        state: &mut DbscanState,
        labels: &mut [i32],
    ) {
        let mut i = 0;
        while i < seeds.len() {
            let neighbor_idx = seeds[i];
            i += 1;

            if labels[neighbor_idx] == -1 {
                // Was noise or unvisited
                labels[neighbor_idx] = cluster_id;
            }

            if state.visited[neighbor_idx] {
                continue;
            }
            state.visited[neighbor_idx] = true;
            state.hits_processed += 1;

            let neighbors = self.find_neighbors(hits, neighbor_idx, epsilon_sq, window_tof, state);

            if neighbors.len() >= self.config.min_points {
                state.point_types[neighbor_idx] = PointType::Core;
                seeds.extend(neighbors);
            } else {
                state.point_types[neighbor_idx] = PointType::Border;
            }
        }
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
        if hits.is_empty() {
            return Ok(0);
        }

        let n = hits.len();

        // Reset state
        state.hits_processed = 0;
        state.clusters_found = 0;
        state.noise_points = 0;
        state.visited.clear();
        state.visited.resize(n, false);
        state.point_types.clear();
        state.point_types.resize(n, PointType::Undefined);
        state.spatial_grid.clear();

        // Initialize labels
        labels.iter_mut().for_each(|l| *l = -1);

        // Build spatial index
        for (i, hit) in hits.iter().enumerate() {
            state.spatial_grid.insert(hit.x() as i32, hit.y() as i32, i);
        }

        let epsilon_sq = self.config.epsilon * self.config.epsilon;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;
        let mut cluster_id = 0;

        for i in 0..n {
            if state.visited[i] {
                continue;
            }
            state.visited[i] = true;

            let neighbors = self.find_neighbors(hits, i, epsilon_sq, window_tof, state);

            if neighbors.len() < self.config.min_points {
                state.point_types[i] = PointType::Noise;
                state.noise_points += 1;
            } else {
                state.point_types[i] = PointType::Core;
                labels[i] = cluster_id;
                self.expand_cluster(
                    hits, i, neighbors, cluster_id, epsilon_sq, window_tof, state, labels,
                );
                cluster_id += 1;
            }
            state.hits_processed += 1;
        }

        state.clusters_found = cluster_id as usize;
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
        let mut state = DbscanState {
            hits_processed: 100,
            clusters_found: 10,
            noise_points: 5,
            ..Default::default()
        };
        state.reset();
        assert_eq!(state.hits_processed, 0);
        assert_eq!(state.clusters_found, 0);
        assert_eq!(state.noise_points, 0);
    }

    #[test]
    fn test_dbscan_clustering_basic() {
        use rustpix_core::hit::GenericHit;

        let clustering = DbscanClustering::default();
        let mut state = clustering.create_state();

        // Create two clusters of hits
        let hits = vec![
            // Cluster 1: (10, 10)
            GenericHit {
                x: 10,
                y: 10,
                tof: 100,
                ..Default::default()
            },
            GenericHit {
                x: 11,
                y: 11,
                tof: 100,
                ..Default::default()
            },
            GenericHit {
                x: 12,
                y: 12,
                tof: 100,
                ..Default::default()
            },
            // Noise point
            GenericHit {
                x: 100,
                y: 100,
                tof: 100,
                ..Default::default()
            },
            // Cluster 2: (50, 50)
            GenericHit {
                x: 50,
                y: 50,
                tof: 100,
                ..Default::default()
            },
            GenericHit {
                x: 51,
                y: 51,
                tof: 100,
                ..Default::default()
            },
            GenericHit {
                x: 52,
                y: 52,
                tof: 100,
                ..Default::default()
            },
        ];

        let mut labels = vec![0; hits.len()];
        let count = clustering.cluster(&hits, &mut state, &mut labels).unwrap();

        assert_eq!(count, 2);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[1], labels[2]);
        assert_eq!(labels[3], -1); // Noise
        assert_eq!(labels[4], labels[5]);
        assert_eq!(labels[5], labels[6]);
        assert_ne!(labels[0], labels[4]);
    }
}
