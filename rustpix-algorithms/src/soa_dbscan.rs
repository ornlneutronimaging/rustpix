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
        if n == 0 {
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

        // Sort hits within cells by TOF for temporal search?
        // Or just iterate.
        // For DBSCAN, we need to find all neighbors.

        let epsilon_sq = self.config.epsilon * self.config.epsilon;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;

        let mut current_cluster_id = 0;
        let mut visited = vec![false; n];
        let mut noise = vec![false; n]; // or just leave cluster_id = -1

        // DBSCAN is hard to parallelize fully efficiently without complex merge steps.
        // Standard sequential DBSCAN:

        for i in 0..n {
            if visited[i] {
                continue;
            }
            visited[i] = true;

            let neighbors =
                self.region_query(i, batch, &grid, cell_size, grid_w, epsilon_sq, window_tof);

            if neighbors.len() < self.config.min_points {
                noise[i] = true;
            } else {
                batch.cluster_id[i] = current_cluster_id;
                self.expand_cluster(
                    i,
                    neighbors,
                    current_cluster_id,
                    batch,
                    &grid,
                    cell_size,
                    grid_w,
                    epsilon_sq,
                    window_tof,
                    &mut visited,
                    &mut noise,
                );
                current_cluster_id += 1;
            }
        }

        Ok(current_cluster_id as usize)
    }

    fn region_query(
        &self,
        idx: usize,
        batch: &HitBatch,
        grid: &[Vec<usize>],
        cell_size: usize,
        grid_w: usize,
        eps_sq: f64,
        window_tof: u32,
    ) -> Vec<usize> {
        let x = batch.x[idx] as f64;
        let y = batch.y[idx] as f64;
        let tof = batch.tof[idx];
        let cx = batch.x[idx] as usize / cell_size;
        let cy = batch.y[idx] as usize / cell_size;

        let mut neighbors = Vec::new();

        // Check neighboring cells
        for dy in -1..=1 {
            for dx in -1..=1 {
                let ncx = cx as isize + dx;
                let ncy = cy as isize + dy;

                if ncx >= 0 && ncy >= 0 {
                    let gidx = (ncy as usize) * grid_w + (ncx as usize);
                    if let Some(cell) = grid.get(gidx) {
                        for &j in cell {
                            let val_x = batch.x[j] as f64;
                            let val_y = batch.y[j] as f64;
                            let val_tof = batch.tof[j];

                            let dt = if tof > val_tof {
                                tof - val_tof
                            } else {
                                val_tof - tof
                            };
                            if dt <= window_tof {
                                let dist_sq = (x - val_x).powi(2) + (y - val_y).powi(2);
                                if dist_sq <= eps_sq {
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
        _root: usize,
        mut seeds: Vec<usize>,
        cluster_id: i32,
        batch: &mut HitBatch,
        grid: &[Vec<usize>],
        cell_size: usize,
        grid_w: usize,
        eps_sq: f64,
        window_tof: u32,
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

                let neighbors = self.region_query(
                    current_p, batch, grid, cell_size, grid_w, eps_sq, window_tof,
                );
                if neighbors.len() >= self.config.min_points {
                    // Optimized: avoid adding duplicates?
                    // seeds.append(&mut neighbors);
                    // To avoid infinite loops/dups in seeds, we rely on visited check?
                    // DBSCAN standard algo says append.
                    for n in neighbors {
                        // If not already in seeds?
                        // Visited check handles processing, but `seeds` list might grow with duplicates if we aren't careful?
                        // Actually standard DBSCAN: if P is not visited...
                        // We check visited above.
                        // But neighbors might include already visited nodes?
                        // region_query returns all neighbors.
                        // If a neighbor is already in seeds but not processed, we duplicate?
                        // We can use a bitset or just rely on visited check inside loop.
                        // The loop iterates seeds. If seeds contains repeats, visited check skips them.
                        seeds.push(n);
                    }
                }
            } else if batch.cluster_id[current_p] == -1 {
                // If it was visited but not part of a cluster yet (shouldn't happen if logic is correct for Noise handling)
                batch.cluster_id[current_p] = cluster_id;
            }
        }
    }
}
