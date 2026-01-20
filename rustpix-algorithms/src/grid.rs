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
#[derive(Default)]
pub struct GridState {
    pub hits_processed: usize,
    pub clusters_found: usize,
}

impl ClusteringState for GridState {
    fn reset(&mut self) {
        self.hits_processed = 0;
        self.clusters_found = 0;
    }
}

/// Grid-based clustering with spatial indexing.
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
        if hits.is_empty() {
            return Ok(0);
        }

        let n = hits.len();

        // Reset state
        state.hits_processed = 0;
        state.clusters_found = 0;

        // Initialize labels
        labels.iter_mut().for_each(|l| *l = -1);

        // Build spatial index
        // Note: Using a fixed grid size for now, but could be dynamic based on detector size
        // The SpatialGrid uses a HashMap, so the width/height args are just hints/unused
        let mut grid: SpatialGrid<usize> = SpatialGrid::new(self.config.cell_size, 512, 512);

        for (i, hit) in hits.iter().enumerate() {
            grid.insert(hit.x() as i32, hit.y() as i32, i);
        }

        // Union-Find structure
        let mut parent: Vec<usize> = (0..n).collect();
        let mut rank: Vec<usize> = vec![0; n];

        // We can't easily parallelize the Union-Find operations directly.
        // Strategy:
        // 1. Find all edges (pairs of connected hits) in parallel.
        // 2. Perform union operations sequentially.

        use rayon::prelude::*;

        let radius_sq = self.config.radius * self.config.radius;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;

        let cell_size = self.config.cell_size as i32;

        let find_edges_in_cell =
            |i: usize, hit: &H, cell: &[usize], edges: &mut Vec<(usize, usize)>| {
                // Find start index: first element > i
                // Since hits are sorted by TOF and processed in order, indices in cell are sorted.
                let start = cell.partition_point(|&idx| idx <= i);

                // Find end index: first element where tof > limit
                let limit_tof = hit.tof().saturating_add(window_tof);
                // We only search in the slice starting from `start`
                let end_rel = cell[start..].partition_point(|&idx| hits[idx].tof() <= limit_tof);
                let end = start + end_rel;

                for &neighbor_idx in &cell[start..end] {
                    let neighbor = &hits[neighbor_idx];
                    if hit.distance_squared(neighbor) <= radius_sq {
                        edges.push((i, neighbor_idx));
                    }
                }
            };

        let edges: Vec<(usize, usize)> = if self.config.parallel {
            hits.par_iter()
                .enumerate()
                .map_init(
                    || Vec::with_capacity(16),
                    |local_edges, (i, hit)| {
                        local_edges.clear();
                        let x = hit.x() as i32;
                        let y = hit.y() as i32;

                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let px = x + dx * cell_size;
                                let py = y + dy * cell_size;
                                if let Some(cell) = grid.get_cell_slice(px, py) {
                                    find_edges_in_cell(i, hit, cell, local_edges);
                                }
                            }
                        }
                        local_edges.clone()
                    },
                )
                .flatten()
                .collect()
        } else {
            // Sequential fallback
            let mut edge_list = Vec::new();
            for (i, hit) in hits.iter().enumerate() {
                let x = hit.x() as i32;
                let y = hit.y() as i32;

                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let px = x + dx * cell_size;
                        let py = y + dy * cell_size;
                        if let Some(cell) = grid.get_cell_slice(px, py) {
                            find_edges_in_cell(i, hit, cell, &mut edge_list);
                        }
                    }
                }
            }
            edge_list
        };

        let find = |parent: &mut Vec<usize>, mut i: usize| -> usize {
            while i != parent[i] {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        };

        let union = |parent: &mut Vec<usize>, rank: &mut Vec<usize>, i: usize, j: usize| {
            let root_i = find(parent, i);
            let root_j = find(parent, j);
            if root_i != root_j {
                if rank[root_i] < rank[root_j] {
                    parent[root_i] = root_j;
                } else {
                    parent[root_j] = root_i;
                    if rank[root_i] == rank[root_j] {
                        rank[root_i] += 1;
                    }
                }
            }
        };

        // Process edges
        for (u, v) in edges {
            union(&mut parent, &mut rank, u, v);
        }

        // Count cluster sizes
        let mut cluster_sizes = std::collections::HashMap::new();
        for i in 0..n {
            let root = find(&mut parent, i);
            *cluster_sizes.entry(root).or_insert(0) += 1;
        }

        // Assign cluster labels
        let mut root_to_label = std::collections::HashMap::new();
        let mut next_label = 0;

        for (i, label) in labels.iter_mut().enumerate() {
            let root = find(&mut parent, i);
            let size = *cluster_sizes.get(&root).unwrap_or(&0);

            if size < self.config.min_cluster_size as usize {
                *label = -1; // Noise
            } else {
                let label_id = *root_to_label.entry(root).or_insert_with(|| {
                    let l = next_label;
                    next_label += 1;
                    l
                });
                *label = label_id;
            }
        }

        state.hits_processed = n;
        state.clusters_found = next_label as usize;

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
        let mut state = GridState {
            hits_processed: 100,
            clusters_found: 10,
        };
        state.reset();
        assert_eq!(state.hits_processed, 0);
        assert_eq!(state.clusters_found, 0);
    }

    #[test]
    fn test_grid_with_parallel() {
        let algo = GridClustering::default().with_parallel(false);
        assert!(!algo.config.parallel);
    }

    #[test]
    fn test_grid_clustering_basic() {
        use rustpix_core::hit::GenericHit;

        let clustering = GridClustering::default();
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
        ];

        let mut labels = vec![0; hits.len()];
        let count = clustering.cluster(&hits, &mut state, &mut labels).unwrap();

        assert_eq!(count, 2);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[2], labels[3]);
        assert_ne!(labels[0], labels[2]);
    }
}
