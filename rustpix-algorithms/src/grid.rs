//! SoA-based Grid Clustering.
//!
//! Adapted from generic `GridClustering` to work directly on `HitBatch` (`SoA`).

use crate::SpatialGrid;
use rustpix_core::clustering::ClusteringError;
use rustpix_core::soa::HitBatch;

/// Configuration for grid-based clustering.
#[derive(Clone, Debug)]
pub struct GridConfig {
    /// Spatial radius for neighbor detection (pixels).
    pub radius: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
    /// Maximum cluster size (None = unlimited).
    pub max_cluster_size: Option<usize>,
    /// Grid cell size (pixels).
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
/// Reusable grid clustering state.
pub struct GridState {
    /// Number of hits processed.
    pub hits_processed: usize,
    /// Number of clusters found.
    pub clusters_found: usize,
    grid: Option<SpatialGrid<usize>>,
    parent: Vec<usize>,
    rank: Vec<usize>,
    roots: Vec<usize>,
    cluster_sizes: Vec<usize>,
    root_to_label: Vec<i32>,
}

/// SoA-optimized grid clustering implementation.
pub struct GridClustering {
    config: GridConfig,
}

struct GridUnionContext {
    radius_sq: f64,
    window_tof: u32,
    cell_size: i32,
}

impl GridClustering {
    /// Create with custom configuration.
    #[must_use]
    pub fn new(config: GridConfig) -> Self {
        Self { config }
    }

    /// Cluster a batch of hits in-place.
    ///
    /// Updates `cluster_id` field in `batch`.
    ///
    /// # Errors
    /// Returns an error if clustering fails.
    pub fn cluster(
        &self,
        batch: &mut HitBatch,
        state: &mut GridState,
    ) -> Result<usize, ClusteringError> {
        if batch.is_empty() {
            return Ok(0);
        }

        let n = batch.len();
        let GridState {
            hits_processed,
            clusters_found,
            grid,
            parent,
            rank,
            roots,
            cluster_sizes,
            root_to_label,
        } = state;

        *hits_processed = 0;
        *clusters_found = 0;
        batch.cluster_id.fill(-1);

        let (width, height) = Self::batch_dimensions(batch);
        Self::init_union_find(parent, rank, roots, cluster_sizes, root_to_label, n);

        let grid = Self::prepare_grid(grid, self.config.cell_size, width, height);
        Self::fill_grid(grid, batch);

        let union_ctx = GridUnionContext {
            radius_sq: self.config.radius * self.config.radius,
            window_tof: float_to_u32((self.config.temporal_window_ns / 25.0).ceil()),
            cell_size: i32::try_from(self.config.cell_size).unwrap_or(i32::MAX),
        };

        Self::union_hits(batch, grid, parent, rank, n, &union_ctx);

        let clusters = Self::assign_labels(
            batch,
            parent,
            roots,
            cluster_sizes,
            root_to_label,
            n,
            usize::from(self.config.min_cluster_size),
        );

        *hits_processed = n;
        *clusters_found = clusters;
        Ok(clusters)
    }
}

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

impl GridClustering {
    fn batch_dimensions(batch: &HitBatch) -> (usize, usize) {
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        for i in 0..batch.len() {
            let x = usize::from(batch.x[i]);
            let y = usize::from(batch.y[i]);
            if x > max_x {
                max_x = x;
            }
            if y > max_y {
                max_y = y;
            }
        }
        (max_x + 32, max_y + 32)
    }

    fn prepare_grid(
        grid_slot: &mut Option<SpatialGrid<usize>>,
        cell_size: usize,
        width: usize,
        height: usize,
    ) -> &mut SpatialGrid<usize> {
        let grid = grid_slot.get_or_insert_with(|| SpatialGrid::new(cell_size, width, height));
        if grid.cell_size() == cell_size {
            grid.ensure_dimensions(width, height);
            grid.clear();
        } else {
            *grid = SpatialGrid::new(cell_size, width, height);
        }
        grid
    }

    fn fill_grid(grid: &mut SpatialGrid<usize>, batch: &HitBatch) {
        for i in 0..batch.len() {
            grid.insert(i32::from(batch.x[i]), i32::from(batch.y[i]), i);
        }
    }

    fn init_union_find(
        parent: &mut Vec<usize>,
        rank: &mut Vec<usize>,
        roots: &mut Vec<usize>,
        cluster_sizes: &mut Vec<usize>,
        root_to_label: &mut Vec<i32>,
        n: usize,
    ) {
        if parent.len() < n {
            parent.resize(n, 0);
        }
        if rank.len() < n {
            rank.resize(n, 0);
        }
        if roots.len() < n {
            roots.resize(n, 0);
        }
        if cluster_sizes.len() < n {
            cluster_sizes.resize(n, 0);
        }
        if root_to_label.len() < n {
            root_to_label.resize(n, -1);
        }
        for i in 0..n {
            parent[i] = i;
            rank[i] = 0;
        }
    }

    fn union_hits(
        batch: &HitBatch,
        grid: &SpatialGrid<usize>,
        parent: &mut [usize],
        rank: &mut [usize],
        n: usize,
        ctx: &GridUnionContext,
    ) {
        for i in 0..n {
            let x = i32::from(batch.x[i]);
            let y = i32::from(batch.y[i]);

            for dy in -1..=1 {
                for dx in -1..=1 {
                    let px = x + dx * ctx.cell_size;
                    let py = y + dy * ctx.cell_size;

                    if let Some(cell) = grid.get_cell_slice(px, py) {
                        let start = cell.partition_point(|&idx| idx <= i);

                        for &j in &cell[start..] {
                            let dt = batch.tof[j].wrapping_sub(batch.tof[i]);
                            if dt > ctx.window_tof {
                                break;
                            }

                            let dx = f64::from(batch.x[i]) - f64::from(batch.x[j]);
                            let dy = f64::from(batch.y[i]) - f64::from(batch.y[j]);
                            let dist_sq = dx * dx + dy * dy;

                            if dist_sq <= ctx.radius_sq {
                                union_sets(parent, rank, i, j);
                            }
                        }
                    }
                }
            }
        }
    }

    fn assign_labels(
        batch: &mut HitBatch,
        parent: &mut [usize],
        roots: &mut [usize],
        cluster_sizes: &mut [usize],
        root_to_label: &mut [i32],
        n: usize,
        min_cluster_size: usize,
    ) -> usize {
        cluster_sizes[..n].fill(0);
        for (i, root_slot) in roots.iter_mut().enumerate().take(n) {
            let root = find(parent, i);
            *root_slot = root;
            cluster_sizes[root] += 1;
        }

        root_to_label[..n].fill(-1);
        let mut next_label = 0;

        for (i, &root) in roots.iter().enumerate().take(n) {
            let size = cluster_sizes[root];

            if size < min_cluster_size {
                batch.cluster_id[i] = -1;
            } else {
                let label_slot = &mut root_to_label[root];
                if *label_slot < 0 {
                    *label_slot = next_label;
                    next_label += 1;
                }
                batch.cluster_id[i] = *label_slot;
            }
        }

        usize::try_from(next_label).unwrap_or(0)
    }
}

fn float_to_u32(value: f64) -> u32 {
    if value <= 0.0 {
        return 0;
    }
    if value >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    format!("{value:.0}").parse::<u32>().unwrap_or(u32::MAX)
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
        batch.push((10, 10, 100, 5, 0, 0));
        batch.push((11, 11, 102, 5, 0, 0)); // Close in space and time

        // Cluster 2
        batch.push((50, 50, 100, 5, 0, 0)); // Far in space

        // Noise
        batch.push((100, 100, 10000, 5, 0, 0)); // Far in time

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
        batch.push((10, 10, 100, 5, 0, 0)); // Hit A
        batch.push((10, 10, 200, 5, 0, 0)); // Hit B (far future)
        batch.push((10, 10, 102, 5, 0, 0)); // Hit C (should be with A)

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
        batch.push((10, 10, 100, 5, 0, 0));
        batch.push((10, 10, 101, 5, 0, 0)); // Linked to 0 (delta 1 tick = 25ns < 50ns)
        batch.push((10, 10, 200, 5, 0, 0)); // Not linked

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
