//! SoA-optimized DBSCAN clustering.

use rayon::prelude::*;
use rustpix_core::clustering::ClusteringError;
use rustpix_core::soa::HitBatch;

/// Configuration for DBSCAN clustering.
#[derive(Clone, Debug)]
pub struct DbscanConfig {
    /// Spatial neighborhood radius (pixels).
    pub epsilon: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
    /// Minimum number of points to seed a cluster.
    pub min_points: usize,
    /// Minimum cluster size to keep after pruning.
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

/// DBSCAN clustering implementation.
pub struct DbscanClustering {
    config: DbscanConfig,
}

#[derive(Default)]
/// Reusable DBSCAN clustering state buffers.
pub struct DbscanState {
    grid: Vec<Vec<usize>>,
    visited: Vec<bool>,
    noise: Vec<bool>,
    neighbors: Vec<usize>,
    seeds: Vec<usize>,
    cluster_sizes: Vec<usize>,
    id_map: Vec<i32>,
}

struct DbscanContext<'a> {
    grid: &'a [Vec<usize>],
    cell_size: usize,
    grid_w: usize,
    eps_sq: f64,
    window_tof: u32,
}

/// Mutable tracking state used during DBSCAN clustering.
struct TrackingState<'a> {
    visited: &'a mut [bool],
    noise: &'a mut [bool],
}

impl DbscanClustering {
    /// Create a DBSCAN clustering instance with the provided configuration.
    #[must_use]
    pub fn new(config: DbscanConfig) -> Self {
        Self { config }
    }

    /// Create a fresh DBSCAN state container.
    #[must_use]
    pub fn create_state(&self) -> DbscanState {
        DbscanState::default()
    }

    /// Cluster hits using DBSCAN.
    ///
    /// # Errors
    /// Returns an error if clustering fails.
    pub fn cluster(
        &self,
        batch: &mut HitBatch,
        state: &mut DbscanState,
    ) -> Result<usize, ClusteringError> {
        let n = batch.len();
        if batch.is_empty() {
            return Ok(0);
        }

        // Reset cluster IDs
        // SoA cluster_id is i32
        batch.cluster_id.par_iter_mut().for_each(|id| *id = -1);

        // We need spatial indexing.
        // Reuse logic from SoAGridClustering or implement a simple grid?
        // DBSCAN needs precise distance check, so Grid is just a broad phase.

        let ctx = self.build_context(batch, &mut state.grid);

        if state.visited.len() < n {
            state.visited.resize(n, false);
            state.noise.resize(n, false);
        }
        // Reset flags
        state.visited[..n].fill(false);
        state.noise[..n].fill(false);

        let mut current_cluster_id = 0;

        // Use slices for tracking to avoid split borrowing issues with state
        // We'll pass slices to helper functions
        // But we need to use the `visited` and `noise` from `state`.
        // To avoid borrowing `state` while reading `ctx` (which borrows `state.grid`),
        // we can split `state` or pass things differently.
        // `ctx` borrows `state.grid`.
        // `visited` and `noise` are separate fields.
        // Rust might figure it out if we borrow fields separately.

        // To make it safe and easier, let's extract the slices from state:
        let visited_slice = &mut state.visited[..n];
        let noise_slice = &mut state.noise[..n];
        let neighbors_buffer = &mut state.neighbors;
        let seeds_buffer = &mut state.seeds;

        for i in 0..n {
            if visited_slice[i] {
                continue;
            }
            visited_slice[i] = true;

            Self::region_query_into(&ctx, i, batch, neighbors_buffer);

            if neighbors_buffer.len() < self.config.min_points {
                noise_slice[i] = true;
            } else {
                batch.cluster_id[i] = current_cluster_id;
                seeds_buffer.clear();
                seeds_buffer.extend_from_slice(neighbors_buffer);
                let mut tracking = TrackingState {
                    visited: visited_slice,
                    noise: noise_slice,
                };
                self.expand_cluster(
                    &ctx,
                    seeds_buffer,
                    current_cluster_id,
                    batch,
                    &mut tracking,
                    neighbors_buffer,
                );
                current_cluster_id += 1;
            }
        }

        Ok(self.prune_small_clusters(batch, state, current_cluster_id))
    }

    fn build_context<'a>(
        &self,
        batch: &HitBatch,
        grid: &'a mut Vec<Vec<usize>>,
    ) -> DbscanContext<'a> {
        let n = batch.len();
        let cell_size = float_to_usize(self.config.epsilon.ceil()).max(32);

        let mut max_x = 0usize;
        let mut max_y = 0usize;
        for i in 0..n {
            let x = usize::from(batch.x[i]);
            let y = usize::from(batch.y[i]);
            if x > max_x {
                max_x = x;
            }
            if y > max_y {
                max_y = y;
            }
        }

        let width = max_x + 32;
        let height = max_y + 32;
        let grid_w = width / cell_size + 1;
        let grid_h = height / cell_size + 1;
        let total_cells = grid_w * grid_h;

        if grid.len() < total_cells {
            grid.resize(total_cells, Vec::new());
        } else {
            for cell in grid.iter_mut() {
                cell.clear();
            }
        }

        for i in 0..n {
            let cx = usize::from(batch.x[i]) / cell_size;
            let cy = usize::from(batch.y[i]) / cell_size;
            let idx = cy * grid_w + cx;
            if idx < grid.len() {
                grid[idx].push(i);
            }
        }

        let epsilon_sq = self.config.epsilon * self.config.epsilon;
        let window_tof = float_to_u32((self.config.temporal_window_ns / 25.0).ceil());

        DbscanContext {
            grid,
            cell_size,
            grid_w,
            eps_sq: epsilon_sq,
            window_tof,
        }
    }

    fn prune_small_clusters(
        &self,
        batch: &mut HitBatch,
        state: &mut DbscanState,
        cluster_count: i32,
    ) -> usize {
        if self.config.min_cluster_size <= 1 || cluster_count <= 0 {
            return usize::try_from(cluster_count).unwrap_or(0);
        }

        let current_cluster_len = usize::try_from(cluster_count).unwrap_or(0);
        if state.cluster_sizes.len() < current_cluster_len {
            state.cluster_sizes.resize(current_cluster_len, 0);
        }
        let sizes = &mut state.cluster_sizes[..current_cluster_len];
        sizes.fill(0);
        for &id in &batch.cluster_id {
            if let Ok(idx) = usize::try_from(id) {
                if let Some(size) = sizes.get_mut(idx) {
                    *size += 1;
                }
            }
        }

        if state.id_map.len() < current_cluster_len {
            state.id_map.resize(current_cluster_len, -1);
        }
        let id_map = &mut state.id_map[..current_cluster_len];
        id_map.fill(-1);
        let mut new_cluster_count = 0;
        let min_size = usize::from(self.config.min_cluster_size);

        for (old_id, &size) in sizes.iter().enumerate() {
            if size >= min_size {
                id_map[old_id] = new_cluster_count;
                new_cluster_count += 1;
            }
        }

        batch.cluster_id.par_iter_mut().for_each(|id| {
            if let Ok(idx) = usize::try_from(*id) {
                if let Some(&new_id) = id_map.get(idx) {
                    *id = new_id;
                }
            }
        });

        usize::try_from(new_cluster_count).unwrap_or(0)
    }

    fn region_query_into(
        ctx: &DbscanContext,
        idx: usize,
        batch: &HitBatch,
        neighbors: &mut Vec<usize>,
    ) {
        let x = f64::from(batch.x[idx]);
        let y = f64::from(batch.y[idx]);
        let tof = batch.tof[idx];
        let cx = usize::from(batch.x[idx]) / ctx.cell_size;
        let cy = usize::from(batch.y[idx]) / ctx.cell_size;
        let cell_col = i32::try_from(cx).unwrap_or(i32::MAX);
        let cell_row = i32::try_from(cy).unwrap_or(i32::MAX);

        neighbors.clear();

        // Check neighboring cells
        for dy in -1..=1 {
            for dx in -1..=1 {
                let ncx = cell_col + dx;
                let ncy = cell_row + dy;
                if ncx < 0 || ncy < 0 {
                    continue;
                }
                let (Ok(neighbor_x), Ok(neighbor_y)) = (usize::try_from(ncx), usize::try_from(ncy))
                else {
                    continue;
                };
                let gidx = neighbor_y * ctx.grid_w + neighbor_x;
                if let Some(cell) = ctx.grid.get(gidx) {
                    for &j in cell {
                        if j == idx {
                            continue;
                        }
                        let val_x = f64::from(batch.x[j]);
                        let val_y = f64::from(batch.y[j]);
                        let val_tof = batch.tof[j];

                        let dt = tof.abs_diff(val_tof);
                        if dt <= ctx.window_tof {
                            let dist_sq = (x - val_x).powi(2) + (y - val_y).powi(2);
                            if dist_sq <= ctx.eps_sq {
                                neighbors.push(j);
                            }
                        }
                    }
                }
            }
        }
    }

    fn expand_cluster(
        &self,
        ctx: &DbscanContext,
        seeds: &mut Vec<usize>,
        cluster_id: i32,
        batch: &mut HitBatch,
        tracking: &mut TrackingState,
        neighbors: &mut Vec<usize>,
    ) {
        let mut i = 0;
        while i < seeds.len() {
            let current_p = seeds[i];
            i += 1;

            if tracking.noise[current_p] {
                tracking.noise[current_p] = false;
                batch.cluster_id[current_p] = cluster_id;
            }

            if !tracking.visited[current_p] {
                tracking.visited[current_p] = true;
                batch.cluster_id[current_p] = cluster_id;

                Self::region_query_into(ctx, current_p, batch, neighbors);
                if neighbors.len() >= self.config.min_points {
                    seeds.extend_from_slice(neighbors);
                }
            } else if batch.cluster_id[current_p] == -1 {
                batch.cluster_id[current_p] = cluster_id;
            }
        }
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

fn float_to_usize(value: f64) -> usize {
    if value <= 0.0 {
        return 0;
    }
    format!("{value:.0}").parse::<usize>().unwrap_or(usize::MAX)
}
