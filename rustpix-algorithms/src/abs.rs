//! SoA-optimized ABS (Age-Based Spatial) clustering.

use crate::params::{float_to_usize, require_finite_non_negative, window_ticks};
use rustpix_core::clustering::ClusteringError;
use rustpix_core::soa::HitBatch;

const MIN_CELL_SIZE: usize = 32;
/// Largest radius that can change a comparison between `u16` coordinates.
const MAX_RADIUS: usize = 65_535;

/// Configuration for ABS (Age-Based Spatial) clustering.
#[derive(Clone, Debug)]
pub struct AbsConfig {
    /// Distance (pixels) a hit may lie outside an open cluster's bounding box.
    pub radius: f64,
    /// Maximum time (ns) after a cluster's first hit for another hit to join it.
    pub neutron_correlation_window_ns: f64,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
    /// Number of hits between aging scans; must be at least 1.
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
    /// Grid cells covered by the bounding box, each of which lists this bucket.
    cells: CellRange,
}

/// Inclusive block of grid cells.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct CellRange {
    col_min: usize,
    col_max: usize,
    row_min: usize,
    row_max: usize,
}

impl CellRange {
    fn of_point(x: u16, y: u16, cell_size: usize) -> Self {
        let (col, row) = (usize::from(x) / cell_size, usize::from(y) / cell_size);
        Self {
            col_min: col,
            col_max: col,
            row_min: row,
            row_max: row,
        }
    }

    fn contains(&self, col: usize, row: usize) -> bool {
        (self.col_min..=self.col_max).contains(&col) && (self.row_min..=self.row_max).contains(&row)
    }
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
            cells: CellRange::default(),
        }
    }

    fn initialize(&mut self, x: u16, y: u16, tof: u32, cluster_id: i32, cell_size: usize) {
        self.x_min = x;
        self.x_max = x;
        self.y_min = y;
        self.y_max = y;
        self.start_tof = tof;
        self.cluster_id = cluster_id;
        self.is_active = true;
        self.cells = CellRange::of_point(x, y, cell_size);
    }

    fn add_hit(&mut self, x: u16, y: u16) {
        self.x_min = self.x_min.min(x);
        self.x_max = self.x_max.max(x);
        self.y_min = self.y_min.min(y);
        self.y_max = self.y_max.max(y);
    }
}

/// ABS clustering implementation.
pub struct AbsClustering {
    config: AbsConfig,
}

struct AbsSearchContext {
    window_tof: u32,
    cell_size: usize,
    grid_w: usize,
    radius_i32: i32,
}

/// Reusable ABS clustering state for streaming or repeated runs.
pub struct AbsState {
    buckets: Vec<Bucket>,
    active_indices: Vec<usize>,
    free_indices: Vec<usize>,
    /// Row-major cells listing the open buckets whose bounding box covers them.
    grid: Vec<Vec<usize>>,
    grid_w: usize,
    grid_h: usize,
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
            grid_h: 256 / 32 + 1,
            next_cluster_id: 0,
            cluster_sizes: Vec::new(),
        }
    }
}

impl AbsClustering {
    /// Create an ABS clustering instance with the provided configuration.
    #[must_use]
    pub fn new(config: AbsConfig) -> Self {
        Self { config }
    }

    /// Cluster one pulse of TOF-ordered hits using ABS.
    ///
    /// # Errors
    /// Returns an error for a non-finite or negative radius or window, a zero
    /// scan interval, or when internal state limits are exceeded.
    pub fn cluster(
        &self,
        batch: &mut HitBatch,
        state: &mut AbsState,
    ) -> Result<usize, ClusteringError> {
        self.validate()?;
        if batch.is_empty() {
            return Ok(0);
        }
        // Buckets left open by a call that returned an error.
        if !state.active_indices.is_empty() {
            Self::close_active_buckets(state);
            for cell in &mut state.grid {
                cell.clear();
            }
        }

        let n = batch.len();
        // Since batch.cluster_id stores per-hit result, we write to it eventually.
        // ABS writes cluster ID when assigning hits to buckets.
        batch.cluster_id.fill(-1);
        state.cluster_sizes.clear();
        state.next_cluster_id = 0;

        let window_tof = window_ticks(self.config.neutron_correlation_window_ns);
        // At least as wide as the radius, so the 3x3 cell search covers it.
        let radius = float_to_usize(self.config.radius.ceil()).min(MAX_RADIUS);
        let cell_size = MIN_CELL_SIZE.max(radius);

        let grid_w = Self::resize_grid(batch, state, cell_size);
        let radius_i32 = i32::try_from(radius).unwrap_or(i32::MAX);
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
                Self::scan_and_close(tof, state, window_tof, grid_w);
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
                let (col, row) = (usize::from(x) / cell_size, usize::from(y) / cell_size);
                Self::extend_cells(state, bidx, col, row, grid_w);
            } else {
                let bidx = Self::get_bucket(state)?;
                let cid = Self::new_cluster_id(state)?;
                state.buckets[bidx].initialize(x, y, tof, cid, cell_size);
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

        Ok(Self::finish_batch(
            batch,
            state,
            u32::from(self.config.min_cluster_size),
        ))
    }

    fn validate(&self) -> Result<(), ClusteringError> {
        require_finite_non_negative("radius", self.config.radius)?;
        require_finite_non_negative(
            "neutron_correlation_window_ns",
            self.config.neutron_correlation_window_ns,
        )?;
        if self.config.scan_interval == 0 {
            return Err(ClusteringError::InvalidConfig(
                "scan_interval must be at least 1".to_string(),
            ));
        }
        Ok(())
    }

    /// Lists the bucket in the cells its bounding box gains by covering cell `(col, row)`.
    fn extend_cells(state: &mut AbsState, bidx: usize, col: usize, row: usize, grid_w: usize) {
        let known = state.buckets[bidx].cells;
        if known.contains(col, row) {
            return;
        }
        let now = CellRange {
            col_min: known.col_min.min(col),
            col_max: known.col_max.max(col),
            row_min: known.row_min.min(row),
            row_max: known.row_max.max(row),
        };
        for r in now.row_min..=now.row_max {
            for c in now.col_min..=now.col_max {
                if !known.contains(c, r) {
                    if let Some(cell) = state.grid.get_mut(r * grid_w + c) {
                        cell.push(bidx);
                    }
                }
            }
        }
        state.buckets[bidx].cells = now;
    }

    /// Removes the bucket from every cell that lists it.
    fn unregister(state: &mut AbsState, bidx: usize, grid_w: usize) {
        let cells = state.buckets[bidx].cells;
        for row in cells.row_min..=cells.row_max {
            for col in cells.col_min..=cells.col_max {
                if let Some(cell) = state.grid.get_mut(row * grid_w + col) {
                    if let Some(pos) = cell.iter().position(|&listed| listed == bidx) {
                        cell.swap_remove(pos);
                    }
                }
            }
        }
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

        // One empty column and row beyond the last hit's cell, so the 3x3 search
        // never reads past a row's end into the next row.
        let req_grid_w = max_x / cell_size + 2;
        let req_grid_h = max_y / cell_size + 2;

        // Insertion, lookup and removal all use `grid_w` as the row stride. A
        // grid too small in either direction is rebuilt to this batch's size.
        if req_grid_w > state.grid_w || req_grid_h > state.grid_h {
            state.grid_w = req_grid_w;
            state.grid_h = req_grid_h;
            state.grid = vec![Vec::new(); state.grid_w * state.grid_h];
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

    /// Closes every open bucket so clusters never span batches, then marks hits of clusters
    /// below `min_cluster_size` as noise (-1) and renumbers the rest from 0.
    fn finish_batch(batch: &mut HitBatch, state: &mut AbsState, min_cluster_size: u32) -> usize {
        Self::close_active_buckets(state);
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

    fn close_active_buckets(state: &mut AbsState) {
        let active = std::mem::take(&mut state.active_indices);
        for bidx in active {
            Self::unregister(state, bidx, state.grid_w);
            state.buckets[bidx].is_active = false;
            state.free_indices.push(bidx);
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

    fn scan_and_close(ref_tof: u32, state: &mut AbsState, window_tof: u32, grid_w: usize) {
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
            Self::unregister(state, bidx, grid_w);
            state.buckets[bidx].is_active = false;
            state.free_indices.push(bidx);
        }
    }
}
