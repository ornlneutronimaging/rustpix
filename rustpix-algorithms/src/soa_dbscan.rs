//! SoA-optimized DBSCAN clustering.

use rayon::prelude::*;
use rustpix_core::clustering::ClusteringError;
use rustpix_core::soa::HitBatch;

#[derive(Clone, Debug)]
pub struct SoADbscanConfig {
    pub epsilon: f64,
    pub temporal_window_ns: f64,
    pub min_points: usize,
    pub min_cluster_size: u16,
}

impl Default for SoADbscanConfig {
    fn default() -> Self {
        Self {
            epsilon: 5.0,
            temporal_window_ns: 75.0,
            min_points: 2,
            min_cluster_size: 1,
        }
    }
}

pub struct SoADbscanClustering {
    config: SoADbscanConfig,
}

struct DbscanContext<'a> {
    grid: &'a [Vec<usize>],
    cell_size: usize,
    grid_w: usize,
    eps_sq: f64,
    window_tof: u32,
}

impl SoADbscanClustering {
    pub fn new(config: SoADbscanConfig) -> Self {
        Self { config }
    }

    pub fn cluster(
        &self,
        batch: &mut HitBatch,
        // We can use a state struct/buffer to avoid realloc
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

        let width = 256;
        let height = 256;
        let mut grid: Vec<Vec<usize>> =
            vec![Vec::new(); (width / cell_size + 1) * (height / cell_size + 1)];
        let grid_w = width / cell_size + 1;

        // Populate grid
        for i in 0..n {
            let cx = batch.x[i] as usize / cell_size;
            let cy = batch.y[i] as usize / cell_size;
            let idx = cy * grid_w + cx;
            if idx < grid.len() {
                grid[idx].push(i);
            }
        }

        let epsilon_sq = self.config.epsilon * self.config.epsilon;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;

        let ctx = DbscanContext {
            grid: &grid,
            cell_size,
            grid_w,
            eps_sq: epsilon_sq,
            window_tof,
        };

        let mut current_cluster_id = 0;
        let mut visited = vec![false; n];
        let mut noise = vec![false; n]; // or just leave cluster_id = -1

        for i in 0..n {
            if visited[i] {
                continue;
            }
            visited[i] = true;

            let neighbors = self.region_query(&ctx, i, batch);

            if neighbors.len() < self.config.min_points {
                noise[i] = true;
            } else {
                batch.cluster_id[i] = current_cluster_id;
                self.expand_cluster(
                    &ctx,
                    i,
                    neighbors,
                    current_cluster_id,
                    batch,
                    &mut visited,
                    &mut noise,
                );
                current_cluster_id += 1;
            }
        }

        Ok(current_cluster_id as usize)
    }

    fn region_query(&self, ctx: &DbscanContext, idx: usize, batch: &HitBatch) -> Vec<usize> {
        let x = batch.x[idx] as f64;
        let y = batch.y[idx] as f64;
        let tof = batch.tof[idx];
        let cx = batch.x[idx] as usize / ctx.cell_size;
        let cy = batch.y[idx] as usize / ctx.cell_size;

        let mut neighbors = Vec::new();

        // Check neighboring cells
        for dy in -1..=1 {
            for dx in -1..=1 {
                let ncx = cx as isize + dx;
                let ncy = cy as isize + dy;

                if ncx >= 0 && ncy >= 0 {
                    let gidx = (ncy as usize) * ctx.grid_w + (ncx as usize);
                    if let Some(cell) = ctx.grid.get(gidx) {
                        for &j in cell {
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
        neighbors
    }

    fn expand_cluster(
        &self,
        ctx: &DbscanContext,
        _root: usize,
        mut seeds: Vec<usize>,
        cluster_id: i32,
        batch: &mut HitBatch,
        visited: &mut [bool],
        noise: &mut [bool],
    ) {
        let mut i = 0;
        while i < seeds.len() {
            let current_p = seeds[i];
            i += 1;

            if noise[current_p] {
                noise[current_p] = false;
                batch.cluster_id[current_p] = cluster_id;
            }

            if !visited[current_p] {
                visited[current_p] = true;
                batch.cluster_id[current_p] = cluster_id;

                let neighbors = self.region_query(ctx, current_p, batch);
                if neighbors.len() >= self.config.min_points {
                    for n in neighbors {
                        seeds.push(n);
                    }
                }
            } else if batch.cluster_id[current_p] == -1 {
                batch.cluster_id[current_p] = cluster_id;
            }
        }
    }
}
