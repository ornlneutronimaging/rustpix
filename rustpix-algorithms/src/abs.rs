//! SoA-optimized ABS (Age-Based Spatial) clustering.

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

struct AbsSearchContext {
    window_tof: u32,
    cell_size: usize,
    grid_w: usize,
    radius_i32: i32,
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
    #[must_use]
    pub fn new(config: AbsConfig) -> Self {
        Self { config }
    }

    /// Cluster hits using the ABS algorithm.
    ///
    /// # Errors
    /// Returns an error if internal state limits are exceeded.
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

        let window_tof = self.window_tof();
        let cell_size = 32;

        let grid_w = Self::resize_grid(batch, state, cell_size);
        let radius_i32 = self.radius_as_i32();
        let search_ctx = AbsSearchContext {
            window_tof,
            cell_size,
            grid_w,
            radius_i32,
        };

        for i in 0..n {
            let x = batch.x[i];
            let y = batch.y[i];
            let tof = batch.tof[i];

            // Aging
            if i % self.config.scan_interval == 0 && i > 0 {
                Self::scan_and_close(tof, state, window_tof, cell_size, grid_w);
            }

            let found = Self::find_bucket_for_hit(x, y, tof, state, &search_ctx);

            if let Some(bidx) = found {
                let cid = state.buckets[bidx].cluster_id;
                if let Ok(idx) = usize::try_from(cid) {
                    if let Some(size) = state.cluster_sizes.get_mut(idx) {
                        *size += 1;
                    }
                }
                batch.cluster_id[i] = cid;
                state.buckets[bidx].add_hit(x, y);
            } else {
                let bidx = Self::get_bucket(state)?;
                let cid = Self::new_cluster_id(state)?;
                state.buckets[bidx].initialize(x, y, tof, cid);
                if let Ok(idx) = usize::try_from(cid) {
                    if let Some(size) = state.cluster_sizes.get_mut(idx) {
                        *size += 1;
                    }
                }
                batch.cluster_id[i] = cid;
                state.active_indices.push(bidx);

                // Insert into grid
                let cell_col = usize::from(x) / cell_size;
                let cell_row = usize::from(y) / cell_size;
                let gidx = cell_row * grid_w + cell_col;
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
        let min_cluster_size = u32::from(self.config.min_cluster_size);
        Ok(Self::finish_batch(
            batch,
            state,
            window_tof,
            cell_size,
            grid_w,
            last_tof,
            min_cluster_size,
        ))
    }

    fn window_tof(&self) -> u32 {
        let window = (self.config.neutron_correlation_window_ns / 25.0).ceil();
        if window <= 0.0 {
            return 0;
        }
        if window >= f64::from(u32::MAX) {
            return u32::MAX;
        }
        format!("{window:.0}").parse::<u32>().unwrap_or(u32::MAX)
    }

    fn radius_as_i32(&self) -> i32 {
        let radius = self.config.radius.ceil();
        if radius <= 0.0 {
            return 0;
        }
        if radius >= f64::from(i32::MAX) {
            return i32::MAX;
        }
        format!("{radius:.0}").parse::<i32>().unwrap_or(i32::MAX)
    }

    fn resize_grid(batch: &HitBatch, state: &mut AbsState, cell_size: usize) -> usize {
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

        let req_w = max_x + 32;
        let req_h = max_y + 32;
        let req_grid_w = req_w / cell_size + 1;
        let req_grid_h = req_h / cell_size + 1;
        let req_total = req_grid_w * req_grid_h;

        if req_total > state.grid.len() || req_grid_w > state.grid_w {
            state.grid = vec![Vec::new(); req_total];
            state.grid_w = req_grid_w;
        }

        state.grid_w
    }

    fn finalize_clusters(
        batch: &mut HitBatch,
        state: &mut AbsState,
        min_cluster_size: u32,
    ) -> usize {
        let mut remap = vec![-1i32; state.cluster_sizes.len()];
        let mut next = 0i32;
        for (cid, &count) in state.cluster_sizes.iter().enumerate() {
            if count >= min_cluster_size {
                remap[cid] = next;
                next += 1;
            }
        }

        for cid in &mut batch.cluster_id {
            if let Ok(idx) = usize::try_from(*cid) {
                if let Some(&new_id) = remap.get(idx) {
                    *cid = new_id;
                }
            }
        }

        usize::try_from(next).unwrap_or(0)
    }

    fn finish_batch(
        batch: &mut HitBatch,
        state: &mut AbsState,
        window_tof: u32,
        cell_size: usize,
        grid_w: usize,
        last_tof: u32,
        min_cluster_size: u32,
    ) -> usize {
        Self::scan_and_close(
            last_tof.wrapping_add(window_tof + 1),
            state,
            window_tof,
            cell_size,
            grid_w,
        );

        // Force close remaining active
        Self::close_active_buckets(state, cell_size, grid_w);
        Self::finalize_clusters(batch, state, min_cluster_size)
    }

    fn find_bucket_for_hit(
        x: u16,
        y: u16,
        tof: u32,
        state: &AbsState,
        ctx: &AbsSearchContext,
    ) -> Option<usize> {
        let cell_col = usize::from(x) / ctx.cell_size;
        let cell_row = usize::from(y) / ctx.cell_size;
        let cell_col_i32 = i32::try_from(cell_col).unwrap_or(i32::MAX);
        let cell_row_i32 = i32::try_from(cell_row).unwrap_or(i32::MAX);
        let ix = i32::from(x);
        let iy = i32::from(y);

        for dy in -1..=1 {
            for dx in -1..=1 {
                let ncx = cell_col_i32 + dx;
                let ncy = cell_row_i32 + dy;
                if ncx < 0 || ncy < 0 {
                    continue;
                }
                let (Ok(neighbor_x), Ok(neighbor_y)) = (usize::try_from(ncx), usize::try_from(ncy))
                else {
                    continue;
                };
                let gidx = neighbor_y * ctx.grid_w + neighbor_x;
                if let Some(cell) = state.grid.get(gidx) {
                    for &bidx in cell {
                        let bucket = &state.buckets[bidx];
                        if bucket.is_active {
                            let x_min_bound = i32::from(bucket.x_min) - ctx.radius_i32;
                            let x_max_bound = i32::from(bucket.x_max) + ctx.radius_i32;
                            let y_min_bound = i32::from(bucket.y_min) - ctx.radius_i32;
                            let y_max_bound = i32::from(bucket.y_max) + ctx.radius_i32;

                            if ix >= x_min_bound
                                && ix <= x_max_bound
                                && iy >= y_min_bound
                                && iy <= y_max_bound
                            {
                                let dt = tof.wrapping_sub(bucket.start_tof);
                                if dt <= ctx.window_tof {
                                    return Some(bidx);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn close_active_buckets(state: &mut AbsState, cell_size: usize, grid_w: usize) {
        let active = std::mem::take(&mut state.active_indices);
        for bidx in active {
            state.buckets[bidx].is_active = false;
            state.free_indices.push(bidx);
            let b = &state.buckets[bidx];
            let gx = usize::from(b.insertion_x) / cell_size;
            let gy = usize::from(b.insertion_y) / cell_size;
            let gidx = gy * grid_w + gx;
            if let Some(cell) = state.grid.get_mut(gidx) {
                if let Some(pos) = cell.iter().position(|&x| x == bidx) {
                    cell.swap_remove(pos);
                }
            }
        }
    }

    fn get_bucket(state: &mut AbsState) -> Result<usize, ClusteringError> {
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

    fn new_cluster_id(state: &mut AbsState) -> Result<i32, ClusteringError> {
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
            let gx = usize::from(b.insertion_x) / cell_size;
            let gy = usize::from(b.insertion_y) / cell_size;
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
