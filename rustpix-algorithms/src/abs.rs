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

/// Inclusive block of grid cells.
#[derive(Clone, Copy, PartialEq, Eq)]
struct CellRange {
    col_min: usize,
    col_max: usize,
    row_min: usize,
    row_max: usize,
}

impl CellRange {
    fn contains(&self, col: usize, row: usize) -> bool {
        (self.col_min..=self.col_max).contains(&col) && (self.row_min..=self.row_max).contains(&row)
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
    /// Grid cells that hold this bucket's index.
    cells: Option<CellRange>,
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
            cells: None,
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
        self.cells = None;
    }

    fn add_hit(&mut self, x: u16, y: u16) {
        self.x_min = self.x_min.min(x);
        self.x_max = self.x_max.max(x);
        self.y_min = self.y_min.min(y);
        self.y_max = self.y_max.max(y);
    }

    fn release(&mut self) {
        self.is_active = false;
        self.cells = None;
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
    grid_h: usize,
    radius: i32,
}

impl AbsSearchContext {
    /// Cells overlapped by the bucket's bounding box grown by the radius.
    fn reach(&self, bucket: &Bucket) -> CellRange {
        let low =
            |v: u16| usize::try_from(i32::from(v) - self.radius).unwrap_or(0) / self.cell_size;
        let high = |v: u16, cells: usize| {
            (usize::try_from(i32::from(v) + self.radius).unwrap_or(0) / self.cell_size)
                .min(cells.saturating_sub(1))
        };
        CellRange {
            col_min: low(bucket.x_min),
            col_max: high(bucket.x_max, self.grid_w),
            row_min: low(bucket.y_min),
            row_max: high(bucket.y_max, self.grid_h),
        }
    }

    fn accepts(&self, bucket: &Bucket, x: u16, y: u16, tof: u32) -> bool {
        let (ix, iy) = (i32::from(x), i32::from(y));
        bucket.is_active
            && ix >= i32::from(bucket.x_min) - self.radius
            && ix <= i32::from(bucket.x_max) + self.radius
            && iy >= i32::from(bucket.y_min) - self.radius
            && iy <= i32::from(bucket.y_max) + self.radius
            && tof.wrapping_sub(bucket.start_tof) <= self.window_tof
    }
}

/// Reusable ABS clustering state for streaming or repeated runs.
pub struct AbsState {
    buckets: Vec<Bucket>,
    active_indices: Vec<usize>,
    free_indices: Vec<usize>,
    /// Row-major cells; each lists the open buckets a hit in that cell may join.
    grid: Vec<Vec<usize>>,
    grid_w: usize,
    grid_h: usize,
    next_cluster_id: i32,
    cluster_sizes: Vec<u32>,
}

impl Default for AbsState {
    fn default() -> Self {
        let cells_per_side = 256 / MIN_CELL_SIZE + 1;
        Self {
            buckets: Vec::new(),
            active_indices: Vec::new(),
            free_indices: Vec::new(),
            grid: vec![Vec::new(); cells_per_side * cells_per_side],
            grid_w: cells_per_side,
            grid_h: cells_per_side,
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

        batch.cluster_id.fill(-1);
        Self::reset(state);

        let radius = float_to_usize(self.config.radius.ceil()).min(MAX_RADIUS);
        let cell_size = MIN_CELL_SIZE.max(radius);
        let (grid_w, grid_h) = Self::resize_grid(batch, state, cell_size);
        let ctx = AbsSearchContext {
            window_tof: window_ticks(self.config.neutron_correlation_window_ns),
            cell_size,
            grid_w,
            grid_h,
            radius: i32::try_from(radius).unwrap_or(i32::MAX),
        };

        for i in 0..batch.len() {
            let x = batch.x[i];
            let y = batch.y[i];
            let tof = batch.tof[i];

            if i > 0 && i % self.config.scan_interval == 0 {
                Self::scan_and_close(tof, state, &ctx);
            }

            let cid = if let Some(bidx) = Self::find_bucket_for_hit(x, y, tof, state, &ctx) {
                state.buckets[bidx].add_hit(x, y);
                Self::register(state, bidx, &ctx);
                state.buckets[bidx].cluster_id
            } else {
                let cid = Self::new_cluster_id(state)?;
                let bidx = Self::get_bucket(state)?;
                state.buckets[bidx].initialize(x, y, tof, cid);
                state.active_indices.push(bidx);
                Self::register(state, bidx, &ctx);
                cid
            };

            if let Some(size) = usize::try_from(cid)
                .ok()
                .and_then(|idx| state.cluster_sizes.get_mut(idx))
            {
                *size += 1;
            }
            batch.cluster_id[i] = cid;
        }

        // Close every open bucket so clusters never span batches.
        Self::close_active_buckets(state);
        Ok(Self::finalize_clusters(
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

    /// Drops anything a previous call left open, including after an error.
    fn reset(state: &mut AbsState) {
        for bidx in state.active_indices.drain(..) {
            state.buckets[bidx].release();
            state.free_indices.push(bidx);
        }
        for cell in &mut state.grid {
            cell.clear();
        }
        state.cluster_sizes.clear();
        state.next_cluster_id = 0;
    }

    /// Grows the grid to hold every hit's cell and returns its `(width, height)` in cells.
    fn resize_grid(batch: &HitBatch, state: &mut AbsState, cell_size: usize) -> (usize, usize) {
        let max_x = batch.x.iter().copied().max().map_or(0, usize::from);
        let max_y = batch.y.iter().copied().max().map_or(0, usize::from);
        let req_grid_w = max_x / cell_size + 1;
        let req_grid_h = max_y / cell_size + 1;

        if req_grid_w > state.grid_w || req_grid_h > state.grid_h {
            state.grid_w = state.grid_w.max(req_grid_w);
            state.grid_h = state.grid_h.max(req_grid_h);
            state.grid = vec![Vec::new(); state.grid_w * state.grid_h];
        }

        (state.grid_w, state.grid_h)
    }

    /// Lists the bucket in every cell its grown bounding box overlaps.
    fn register(state: &mut AbsState, bidx: usize, ctx: &AbsSearchContext) {
        let known = state.buckets[bidx].cells;
        let reach = ctx.reach(&state.buckets[bidx]);
        if known == Some(reach) {
            return;
        }
        for row in reach.row_min..=reach.row_max {
            for col in reach.col_min..=reach.col_max {
                if known.is_some_and(|cells| cells.contains(col, row)) {
                    continue;
                }
                if let Some(cell) = state.grid.get_mut(row * ctx.grid_w + col) {
                    cell.push(bidx);
                }
            }
        }
        state.buckets[bidx].cells = Some(reach);
    }

    fn unregister(state: &mut AbsState, bidx: usize, grid_w: usize) {
        let Some(cells) = state.buckets[bidx].cells.take() else {
            return;
        };
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

    fn find_bucket_for_hit(
        x: u16,
        y: u16,
        tof: u32,
        state: &AbsState,
        ctx: &AbsSearchContext,
    ) -> Option<usize> {
        let col = usize::from(x) / ctx.cell_size;
        let row = usize::from(y) / ctx.cell_size;
        // The earliest-opened cluster wins, whatever order the cell lists it in.
        state
            .grid
            .get(row * ctx.grid_w + col)?
            .iter()
            .copied()
            .filter(|&bidx| ctx.accepts(&state.buckets[bidx], x, y, tof))
            .min_by_key(|&bidx| state.buckets[bidx].cluster_id)
    }

    fn close_active_buckets(state: &mut AbsState) {
        for bidx in state.active_indices.drain(..) {
            state.buckets[bidx].release();
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

    /// Closes buckets whose first hit is more than one window before `ref_tof`.
    fn scan_and_close(ref_tof: u32, state: &mut AbsState, ctx: &AbsSearchContext) {
        let mut closed = Vec::new();
        let buckets = &state.buckets;
        state.active_indices.retain(|&bidx| {
            let open = ref_tof.wrapping_sub(buckets[bidx].start_tof) <= ctx.window_tof;
            if !open {
                closed.push(bidx);
            }
            open
        });

        for bidx in closed {
            Self::unregister(state, bidx, ctx.grid_w);
            state.buckets[bidx].release();
            state.free_indices.push(bidx);
        }
    }
}
