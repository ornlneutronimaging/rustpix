//! SoA-based Grid Clustering.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::items_after_statements
)]
//!
//! Adapted from generic GridClustering to work directly on HitBatch (SoA).

use crate::SpatialGrid;
use rustpix_core::clustering::ClusteringError;
use rustpix_core::soa::HitBatch;

#[derive(Clone, Debug)]
pub struct GridConfig {
    pub radius: f64,
    pub temporal_window_ns: f64,
    pub min_cluster_size: u16,
    pub max_cluster_size: Option<usize>,
    pub cell_size: usize,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            max_cluster_size: None,
            cell_size: 32,
        }
    }
}

#[derive(Default)]
pub struct GridState {
    pub hits_processed: usize,
    pub clusters_found: usize,
}

/// SoA-optimized Grid Clustering.
pub struct GridClustering {
    config: GridConfig,
}

impl GridClustering {
    /// Create with custom configuration.
    pub fn new(config: GridConfig) -> Self {
        Self { config }
    }

    /// Cluster a batch of hits in-place.
    ///
    /// Updates `cluster_id` field in `batch`.
    pub fn cluster(
        &self,
        batch: &mut HitBatch,
        state: &mut GridState,
    ) -> Result<usize, ClusteringError> {
        if batch.is_empty() {
            return Ok(0);
        }

        let n = batch.len();

        // Reset state
        state.hits_processed = 0;
        state.clusters_found = 0;

        // Initialize labels to -1
        batch.cluster_id.fill(-1);

        // Build spatial index
        let mut max_x = 0;
        let mut max_y = 0;
        for i in 0..n {
            let x = batch.x[i];
            let y = batch.y[i];
            if (x as usize) > max_x {
                max_x = x as usize;
            }
            if (y as usize) > max_y {
                max_y = y as usize;
            }
        }

        // Using dynamic grid size
        let mut grid: SpatialGrid<usize> =
            SpatialGrid::new(self.config.cell_size, max_x + 32, max_y + 32);

        for i in 0..n {
            grid.insert(batch.x[i] as i32, batch.y[i] as i32, i);
        }

        // Union-Find structure
        let mut parent: Vec<usize> = (0..n).collect();
        let mut rank: Vec<usize> = vec![0; n];

        let radius_sq = self.config.radius * self.config.radius;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;
        let cell_size = self.config.cell_size as i32;

        // Union-Find Operations
        fn find(parent: &mut [usize], i: usize) -> usize {
            let mut root = i;
            while root != parent[root] {
                root = parent[root];
            }
            let mut curr = i;
            while curr != root {
                let next = parent[curr];
                parent[curr] = root;
                curr = next;
            }
            root
        }

        fn union_sets(parent: &mut [usize], rank: &mut [usize], i: usize, j: usize) {
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
        }

        // Direct Sequential Clustering (Memory Efficient)
        // We iterate hits and merge with neighbors immediately.
        // This avoids allocating a massive edge list (which causes OOM/Swap on large data).
        for i in 0..n {
            let x = batch.x[i] as i32;
            let y = batch.y[i] as i32;

            for dy in -1..=1 {
                for dx in -1..=1 {
                    let px = x + dx * cell_size;
                    let py = y + dy * cell_size;

                    if let Some(cell) = grid.get_cell_slice(px, py) {
                        // Only check neighbors with index > i (to avoid double checking and self-loop)
                        // Assuming cell indices are sorted because we inserted them in order 0..n
                        let start = cell.partition_point(|&idx| idx <= i);

                        for &j in &cell[start..] {
                            // Temporal Pruning: valid because input is TOF-sorted
                            let dt = batch.tof[j].wrapping_sub(batch.tof[i]);
                            if dt > window_tof {
                                break;
                            }

                            // Inline spatial check
                            let dx = batch.x[i] as f64 - batch.x[j] as f64;
                            let dy = batch.y[i] as f64 - batch.y[j] as f64;
                            let dist_sq = dx * dx + dy * dy;

                            if dist_sq <= radius_sq {
                                union_sets(&mut parent, &mut rank, i, j);
                            }
                        }
                    }
                }
            }
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

        for i in 0..n {
            let root = find(&mut parent, i);
            let size = *cluster_sizes.get(&root).unwrap_or(&0);

            if size < self.config.min_cluster_size as usize {
                batch.cluster_id[i] = -1;
            } else {
                let label_id = *root_to_label.entry(root).or_insert_with(|| {
                    let l = next_label;
                    next_label += 1;
                    l
                });
                batch.cluster_id[i] = label_id;
            }
        }

        state.hits_processed = n;
        state.clusters_found = next_label as usize;

        Ok(state.clusters_found)
    }
}

impl Default for GridClustering {
    fn default() -> Self {
        Self::new(GridConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::soa::HitBatch;

    #[test]
    fn test_soa_clustering() {
        let mut batch = HitBatch::default();
        // Cluster 1
        batch.push(10, 10, 100, 5, 0, 0);
        batch.push(11, 11, 102, 5, 0, 0); // Close in space and time

        // Cluster 2
        batch.push(50, 50, 100, 5, 0, 0); // Far in space

        // Noise
        batch.push(100, 100, 10000, 5, 0, 0); // Far in time

        let algo = GridClustering::default();
        let mut state = GridState::default();

        let count = algo.cluster(&mut batch, &mut state).unwrap();

        assert_eq!(count, 3); // 1, 2, and 3 (noise is usually single-hit cluster if min_size=1)
                              // With default min_cluster_size=1, noise is a cluster.

        assert_eq!(batch.cluster_id[0], batch.cluster_id[1]);
        assert_ne!(batch.cluster_id[0], batch.cluster_id[2]);
    }

    #[test]
    fn test_grid_requires_tof_sorted_input() {
        // This test documents that if hits are not sorted by TOF, clustering might fail to link them
        // if we rely on temporal pruning (break loop early).
        //
        // Example: Hit A (TOF 100), Hit B (TOF 200), Hit C (TOF 102)
        // If stored as [A, B, C], when processing A:
        //   - Check A vs B (diff 100). If window=10, loop breaks.
        //   - A vs C never checked.
        // Result: A not linked to C, even though diff is 2.

        let mut batch = HitBatch::default();
        batch.push(10, 10, 100, 5, 0, 0); // Hit A
        batch.push(10, 10, 200, 5, 0, 0); // Hit B (far future)
        batch.push(10, 10, 102, 5, 0, 0); // Hit C (should be with A)

        // Window of 2 ticks (25ns each): 50ns / 25.0 = 2.0, ceil = 2
        let config = GridConfig {
            temporal_window_ns: 50.0,
            ..Default::default()
        };
        let algo = GridClustering::new(config);
        let mut state = GridState::default();

        // This relies on implementation detail. If we implement pruning, we expect A and C NOT to cluster
        // because B stops the search from A.
        algo.cluster(&mut batch, &mut state).unwrap();

        // If full N^2 check, A and C would cluster.
        // With pruning, they won't.
        assert_ne!(
            batch.cluster_id[0], batch.cluster_id[2],
            "Pruning should prevent linking unsorted hits separated by future hits"
        );
    }

    #[test]
    fn test_grid_temporal_pruning() {
        let mut batch = HitBatch::default();

        // Ensure that we don't scan infinity.
        // A, B, C, D... sorted.
        // A (0), B (100), C (200), D (300). Window = 50.
        // A checks B -> fail, break. A checks C? No.
        // If logic is correct, performance is O(N * window_density) not O(N^2).

        // Correctness check:
        batch.push(10, 10, 100, 5, 0, 0);
        batch.push(10, 10, 101, 5, 0, 0); // Linked to 0 (delta 1 tick = 25ns < 50ns)
        batch.push(10, 10, 200, 5, 0, 0); // Not linked

        let config = GridConfig {
            temporal_window_ns: 50.0, // ~2 ticks
            ..Default::default()
        };
        let algo = GridClustering::new(config);
        let mut state = GridState::default();

        algo.cluster(&mut batch, &mut state).unwrap();

        assert_eq!(batch.cluster_id[0], batch.cluster_id[1]);
        assert_ne!(batch.cluster_id[0], batch.cluster_id[2]);
    }
}
