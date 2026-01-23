//! SoA-optimized DBSCAN clustering.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::explicit_iter_loop,
    clippy::unused_self,
    clippy::missing_errors_doc,
    clippy::pub_underscore_fields,
    clippy::must_use_candidate
)]

use rayon::prelude::*;
use rustpix_core::clustering::ClusteringError;
use rustpix_core::soa::HitBatch;

#[derive(Clone, Debug)]
pub struct DbscanConfig {
    pub epsilon: f64,
    pub temporal_window_ns: f64,
    pub min_points: usize,
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

pub struct DbscanClustering {
    config: DbscanConfig,
}

#[derive(Default)]
pub struct DbscanState {
    grid: Vec<Vec<usize>>,
    visited: Vec<bool>,
    noise: Vec<bool>,
    neighbors: Vec<usize>,
    seeds: Vec<usize>,
    cluster_sizes: Vec<usize>,
    id_map: Vec<i32>,
    pub _grid_capacity_w: usize,
    pub _grid_capacity_h: usize,
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
    pub fn new(config: DbscanConfig) -> Self {
        Self { config }
    }

    pub fn create_state(&self) -> DbscanState {
        DbscanState::default()
    }

    #[allow(clippy::too_many_lines)]
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

        // Cell size should be at least epsilon to optimize neighbor search
        let cell_size = (self.config.epsilon.ceil() as usize).max(32);

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

        let width = max_x + 32;
        let height = max_y + 32;
        let grid_w = width / cell_size + 1;
        let grid_h = height / cell_size + 1;
        let total_cells = grid_w * grid_h;

        // Resize state buffers
        if state.grid.len() < total_cells {
            state.grid.resize(total_cells, Vec::new());
        } else {
            // Clear existing grid
            for cell in &mut state.grid {
                cell.clear();
            }
        }

        // Populate grid
        for i in 0..n {
            let cx = batch.x[i] as usize / cell_size;
            let cy = batch.y[i] as usize / cell_size;
            let idx = cy * grid_w + cx;
            if idx < state.grid.len() {
                state.grid[idx].push(i);
            }
        }

        let epsilon_sq = self.config.epsilon * self.config.epsilon;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;

        let ctx = DbscanContext {
            grid: &state.grid,
            cell_size,
            grid_w,
            eps_sq: epsilon_sq,
            window_tof,
        };

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

            self.region_query_into(&ctx, i, batch, neighbors_buffer);

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

        // Post-processing: Filter clusters smaller than min_cluster_size
        if self.config.min_cluster_size > 1 && current_cluster_id > 0 {
            let current_cluster_len = current_cluster_id as usize;
            if state.cluster_sizes.len() < current_cluster_len {
                state.cluster_sizes.resize(current_cluster_len, 0);
            }
            let sizes = &mut state.cluster_sizes[..current_cluster_len];
            sizes.fill(0);
            for &id in batch.cluster_id.iter() {
                if id >= 0 {
                    sizes[id as usize] += 1;
                }
            }

            if state.id_map.len() < current_cluster_len {
                state.id_map.resize(current_cluster_len, -1);
            }
            let id_map = &mut state.id_map[..current_cluster_len];
            id_map.fill(-1);
            let mut new_cluster_count = 0;
            let min_size = self.config.min_cluster_size as usize;

            for (old_id, &size) in sizes.iter().enumerate() {
                if size >= min_size {
                    id_map[old_id] = new_cluster_count;
                    new_cluster_count += 1;
                }
            }

            // Remap cluster IDs
            // If checking map is cheap, parallel iteration is fine.
            // Note: Parallel iteration on batch.cluster_id is already used above.
            batch.cluster_id.par_iter_mut().for_each(|id| {
                if *id >= 0 {
                    *id = id_map[*id as usize];
                }
            });

            current_cluster_id = new_cluster_count;
        }

        Ok(current_cluster_id as usize)
    }

    fn region_query_into(
        &self,
        ctx: &DbscanContext,
        idx: usize,
        batch: &HitBatch,
        neighbors: &mut Vec<usize>,
    ) {
        let x = batch.x[idx] as f64;
        let y = batch.y[idx] as f64;
        let tof = batch.tof[idx];
        let cx = batch.x[idx] as usize / ctx.cell_size;
        let cy = batch.y[idx] as usize / ctx.cell_size;

        neighbors.clear();

        // Check neighboring cells
        for dy in -1..=1 {
            for dx in -1..=1 {
                let ncx = cx as isize + dx;
                let ncy = cy as isize + dy;

                if ncx >= 0 && ncy >= 0 {
                    let gidx = (ncy as usize) * ctx.grid_w + (ncx as usize);
                    if let Some(cell) = ctx.grid.get(gidx) {
                        for &j in cell {
                            if j == idx {
                                continue;
                            }
                            let val_x = batch.x[j] as f64;
                            let val_y = batch.y[j] as f64;
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

                self.region_query_into(ctx, current_p, batch, neighbors);
                if neighbors.len() >= self.config.min_points {
                    seeds.extend_from_slice(neighbors);
                }
            } else if batch.cluster_id[current_p] == -1 {
                batch.cluster_id[current_p] = cluster_id;
            }
        }
    }
}
