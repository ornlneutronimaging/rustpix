//! SoA-optimized ABS (Age-Based Spatial) clustering.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::explicit_iter_loop,
    clippy::unused_self,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

use rustpix_core::clustering::ClusteringError;
use rustpix_core::soa::HitBatch;

#[derive(Clone, Debug)]
pub struct AbsConfig {
    pub radius: f64,
    pub neutron_correlation_window_ns: f64,
    pub min_cluster_size: u16,
    pub scan_interval: usize,
}

impl Default for AbsConfig {
    fn default() -> Self {
        Self {
            radius: 5.0,
            neutron_correlation_window_ns: 75.0,
            min_cluster_size: 1,
            scan_interval: 100,
        }
    }
}

struct Bucket {
    x_min: u16,
    x_max: u16,
    y_min: u16,
    y_max: u16,
    start_tof: u32,
    cluster_id: i32,
    is_active: bool,
    insertion_x: u16,
    insertion_y: u16,
}

impl Bucket {
    fn new() -> Self {
        Self {
            x_min: u16::MAX,
            x_max: 0,
            y_min: u16::MAX,
            y_max: 0,
            start_tof: 0,
            cluster_id: -1,
            is_active: false,
            insertion_x: 0,
            insertion_y: 0,
        }
    }

    fn initialize(&mut self, x: u16, y: u16, tof: u32, cluster_id: i32) {
        self.x_min = x;
        self.x_max = x;
        self.y_min = y;
        self.y_max = y;
        self.start_tof = tof;
        self.cluster_id = cluster_id;
        self.is_active = true;
        self.insertion_x = x;
        self.insertion_y = y;
    }

    fn add_hit(&mut self, x: u16, y: u16) {
        self.x_min = self.x_min.min(x);
        self.x_max = self.x_max.max(x);
        self.y_min = self.y_min.min(y);
        self.y_max = self.y_max.max(y);
    }
}

pub struct AbsClustering {
    config: AbsConfig,
}

pub struct AbsState {
    buckets: Vec<Bucket>,
    active_indices: Vec<usize>,
    free_indices: Vec<usize>,
    grid: Vec<Vec<usize>>, // Spatial index
    grid_w: usize,
    next_cluster_id: i32,
    cluster_sizes: Vec<u32>,
}

impl Default for AbsState {
    fn default() -> Self {
        Self {
            buckets: Vec::new(),
            active_indices: Vec::new(),
            free_indices: Vec::new(),
            grid: vec![Vec::new(); (256 / 32 + 1) * (256 / 32 + 1)], // 32 is cell size
            grid_w: 256 / 32 + 1,
            next_cluster_id: 0,
            cluster_sizes: Vec::new(),
        }
    }
}

impl AbsClustering {
    pub fn new(config: AbsConfig) -> Self {
        Self { config }
    }

    pub fn cluster(
        &self,
        batch: &mut HitBatch,
        state: &mut AbsState,
    ) -> Result<usize, ClusteringError> {
        if batch.is_empty() {
            return Ok(0);
        }

        // Initialize state if needed (or assume persistent state for streaming?)
        // If streaming, we keep state.
        // But users might want to cluster a single batch.
        // Let's assume persistent state passed in `state`.
        // We only reset `cluster_id` in batch.

        let n = batch.len();
        // Since batch.cluster_id stores per-hit result, we write to it eventually.
        // ABS writes cluster ID when assigning hits to buckets.
        batch.cluster_id.fill(-1);
        state.cluster_sizes.clear();
        state.next_cluster_id = 0;

        let window_tof = (self.config.neutron_correlation_window_ns / 25.0).ceil() as u32;
        let cell_size = 32;

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

        let req_w = max_x + 32;
        let req_h = max_y + 32;
        let req_grid_w = req_w / cell_size + 1;
        let req_grid_h = req_h / cell_size + 1;
        let req_total = req_grid_w * req_grid_h;

        if req_total > state.grid.len() || req_grid_w > state.grid_w {
            state.grid = vec![Vec::new(); req_total];
            state.grid_w = req_grid_w;
        }

        let grid_w = state.grid_w;

        for i in 0..n {
            let x = batch.x[i];
            let y = batch.y[i];
            let tof = batch.tof[i];

            // Aging
            if i % self.config.scan_interval == 0 && i > 0 {
                self.scan_and_close(tof, state, window_tof, cell_size, grid_w);
            }

            // Find compatible bucket
            let cx = x as usize / cell_size;
            let cy = y as usize / cell_size;
            let mut found = None;

            // Search neighbors
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let ncx = cx as isize + dx;
                    let ncy = cy as isize + dy;
                    if ncx >= 0 && ncy >= 0 {
                        let gidx = (ncy as usize) * grid_w + (ncx as usize);
                        if let Some(cell) = state.grid.get(gidx) {
                            for &bidx in cell {
                                let bucket = &state.buckets[bidx];
                                if bucket.is_active {
                                    // Spatial check
                                    // Using int math
                                    let r = self.config.radius.ceil() as i32;
                                    let bx_min = bucket.x_min as i32 - r;
                                    let bx_max = bucket.x_max as i32 + r;
                                    let by_min = bucket.y_min as i32 - r;
                                    let by_max = bucket.y_max as i32 + r;
                                    let ix = x as i32;
                                    let iy = y as i32;

                                    if ix >= bx_min && ix <= bx_max && iy >= by_min && iy <= by_max
                                    {
                                        // Temporal check
                                        let dt = tof.wrapping_sub(bucket.start_tof);
                                        if dt <= window_tof {
                                            found = Some(bidx);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }

            if let Some(bidx) = found {
                let cid = state.buckets[bidx].cluster_id;
                state.cluster_sizes[cid as usize] += 1;
                batch.cluster_id[i] = cid;
                state.buckets[bidx].add_hit(x, y);
            } else {
                let bidx = self.get_bucket(state)?;
                let cid = self.new_cluster_id(state)?;
                state.buckets[bidx].initialize(x, y, tof, cid);
                state.cluster_sizes[cid as usize] += 1;
                batch.cluster_id[i] = cid;
                state.active_indices.push(bidx);

                // Insert into grid
                let gidx = cy * grid_w + cx;
                if gidx < state.grid.len() {
                    state.grid[gidx].push(bidx);
                }
            }
        }

        // Final cleanup?
        // If this is streaming, we DON'T close active buckets at end of batch unless strictly required.
        // But user expects clustering to finish for batch?
        // If we keep state, we might return partial hits?
        // The `cluster` function usually assumes a closed batch.
        // If streaming, we should probably close everything to yield results,
        // OR we yield only closed clusters?
        // The `HitBatch` needs to be fully labeled if we want to extract from it.
        // If we leave buckets open, those hits have cluster_id = -1.

        // For now, force close everything at end of batch to match existing behavior on distinct files.
        // Stream users might want persistence.
        // But `process_section_into_batch` creates isolated batches per chunk.
        // If we want cross-chunk clustering, we need persistent state.
        // I'll close all for now.

        let last_tof = batch.tof.last().copied().unwrap_or(0);
        self.scan_and_close(
            last_tof.wrapping_add(window_tof + 1),
            state,
            window_tof,
            cell_size,
            grid_w,
        );

        // Force close remaining active
        let active = std::mem::take(&mut state.active_indices);
        for bidx in active {
            state.buckets[bidx].is_active = false;
            state.free_indices.push(bidx);
            // Remove from grid? Too expensive to find.
            // Just clear grid at end?
            // Grid clean up relies on insertion coords.
            let b = &state.buckets[bidx];
            let gx = b.insertion_x as usize / cell_size;
            let gy = b.insertion_y as usize / cell_size;
            let gidx = gy * grid_w + gx;
            if let Some(cell) = state.grid.get_mut(gidx) {
                if let Some(pos) = cell.iter().position(|&x| x == bidx) {
                    cell.swap_remove(pos);
                }
            }
        }

        let min_cluster_size = self.config.min_cluster_size as u32;
        let mut remap = vec![-1i32; state.cluster_sizes.len()];
        let mut next = 0i32;
        for (cid, &count) in state.cluster_sizes.iter().enumerate() {
            if count >= min_cluster_size {
                remap[cid] = next;
                next += 1;
            }
        }

        for cid in &mut batch.cluster_id {
            if *cid >= 0 {
                *cid = remap[*cid as usize];
            }
        }

        Ok(next as usize)
    }

    fn get_bucket(&self, state: &mut AbsState) -> Result<usize, ClusteringError> {
        if let Some(idx) = state.free_indices.pop() {
            Ok(idx)
        } else {
            if state.buckets.len() >= 1_000_000 {
                return Err(ClusteringError::StateError(
                    "bucket pool size exceeds limit (1,000,000)".to_string(),
                ));
            }
            let idx = state.buckets.len();
            state.buckets.push(Bucket::new());
            Ok(idx)
        }
    }

    fn new_cluster_id(&self, state: &mut AbsState) -> Result<i32, ClusteringError> {
        if state.next_cluster_id == i32::MAX {
            return Err(ClusteringError::StateError(
                "cluster id overflow".to_string(),
            ));
        }
        let cid = state.next_cluster_id;
        state.next_cluster_id += 1;
        state.cluster_sizes.push(0);
        Ok(cid)
    }

    fn scan_and_close(
        &self,
        ref_tof: u32,
        state: &mut AbsState,
        window_tof: u32,
        cell_size: usize,
        grid_w: usize,
    ) {
        let mut keep = Vec::new();
        let mut remove = Vec::new();

        for &bidx in &state.active_indices {
            let bucket = &state.buckets[bidx];
            let dt = ref_tof.wrapping_sub(bucket.start_tof);
            if dt > window_tof {
                remove.push(bidx);
            } else {
                keep.push(bidx);
            }
        }
        state.active_indices = keep;

        for bidx in remove {
            // Remove from grid
            let b = &state.buckets[bidx];
            let gx = b.insertion_x as usize / cell_size;
            let gy = b.insertion_y as usize / cell_size;
            let gidx = gy * grid_w + gx;
            if let Some(cell) = state.grid.get_mut(gidx) {
                if let Some(pos) = cell.iter().position(|&x| x == bidx) {
                    cell.swap_remove(pos);
                }
            }

            state.buckets[bidx].is_active = false;
            state.free_indices.push(bidx);
        }
    }
}
