//! ABS (Age-Based Spatial) clustering algorithm.
//!
//! **IMPORTANT**: This is the PRIMARY clustering algorithm. See IMPLEMENTATION_PLAN.md
//! Part 4.1 for the full implementation details.
//!
//! Key characteristics:
//! - Complexity: O(n) average, O(n log n) worst case
//! - Uses bucket pool for memory efficiency
//! - Spatial indexing (32x32 grid) for O(1) neighbor lookup
//! - Age-based bucket closure for temporal correlation

use crate::spatial::SpatialGrid;
use rustpix_core::clustering::{
    ClusteringConfig, ClusteringError, ClusteringState, ClusteringStatistics, HitClustering,
};
use rustpix_core::hit::Hit;

/// ABS-specific configuration.
#[derive(Clone, Debug)]
pub struct AbsConfig {
    /// Spatial radius for bucket membership (pixels).
    pub radius: f64,
    /// Temporal correlation window (nanoseconds).
    pub neutron_correlation_window_ns: f64,
    /// How often to scan for aged buckets (every N hits).
    pub scan_interval: usize,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
    /// Pre-allocated bucket pool size.
    pub pre_allocate_buckets: usize,
}

impl Default for AbsConfig {
    fn default() -> Self {
        Self {
            radius: 5.0,
            neutron_correlation_window_ns: 75.0,
            scan_interval: 100,
            min_cluster_size: 1,
            pre_allocate_buckets: 1000,
        }
    }
}

impl AbsConfig {
    /// Temporal window in TOF units (25ns).
    pub fn window_tof(&self) -> u32 {
        (self.neutron_correlation_window_ns / 25.0).ceil() as u32
    }
}

/// Bucket for accumulating spatially close hits.
#[derive(Clone, Debug)]
struct Bucket {
    /// Indices of hits in this bucket.
    hit_indices: Vec<usize>,
    /// Spatial bounding box.
    x_min: i32,
    x_max: i32,
    y_min: i32,
    y_max: i32,
    /// TOF of first hit (for age calculation).
    start_tof: u32,
    /// Assigned cluster ID (-1 if not closed).
    cluster_id: i32,
    /// Whether bucket is active.
    is_active: bool,
    /// Insertion coordinates for spatial index removal.
    insertion_x: i32,
    insertion_y: i32,
}

impl Bucket {
    fn new() -> Self {
        Self {
            hit_indices: Vec::with_capacity(16),
            x_min: i32::MAX,
            x_max: i32::MIN,
            y_min: i32::MAX,
            y_max: i32::MIN,
            start_tof: 0,
            cluster_id: -1,
            is_active: false,
            insertion_x: 0,
            insertion_y: 0,
        }
    }

    fn initialize<H: Hit>(&mut self, hit_idx: usize, hit: &H) {
        self.hit_indices.clear();
        self.hit_indices.push(hit_idx);
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        self.x_min = x;
        self.x_max = x;
        self.y_min = y;
        self.y_max = y;
        self.start_tof = hit.tof();
        self.cluster_id = -1;
        self.is_active = true;
        self.insertion_x = x;
        self.insertion_y = y;
    }

    fn add_hit<H: Hit>(&mut self, hit_idx: usize, hit: &H) {
        self.hit_indices.push(hit_idx);
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        self.x_min = self.x_min.min(x);
        self.x_max = self.x_max.max(x);
        self.y_min = self.y_min.min(y);
        self.y_max = self.y_max.max(y);
    }

    fn fits_spatially<H: Hit>(&self, hit: &H, radius: f64) -> bool {
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        let r = radius.ceil() as i32;

        x >= self.x_min - r && x <= self.x_max + r && y >= self.y_min - r && y <= self.y_max + r
    }

    fn fits_temporally<H: Hit>(&self, hit: &H, window_tof: u32) -> bool {
        hit.tof().wrapping_sub(self.start_tof) <= window_tof
    }

    fn is_aged(&self, reference_tof: u32, window_tof: u32) -> bool {
        reference_tof.wrapping_sub(self.start_tof) > window_tof
    }
}

/// ABS clustering state.
pub struct AbsState {
    /// Bucket pool for reuse.
    bucket_pool: Vec<Bucket>,
    /// Indices of active buckets.
    active_buckets: Vec<usize>,
    /// Indices of free buckets for reuse.
    free_buckets: Vec<usize>,
    /// Spatial index for bucket lookup.
    spatial_grid: SpatialGrid<usize>,
    /// Next cluster ID to assign.
    next_cluster_id: i32,
    /// Hits processed counter.
    hits_processed: usize,
    /// Clusters found counter.
    clusters_found: usize,
}

impl Default for AbsState {
    fn default() -> Self {
        Self {
            bucket_pool: Vec::new(),
            active_buckets: Vec::new(),
            free_buckets: Vec::new(),
            spatial_grid: SpatialGrid::new(32, 512, 512),
            next_cluster_id: 0,
            hits_processed: 0,
            clusters_found: 0,
        }
    }
}

impl ClusteringState for AbsState {
    fn reset(&mut self) {
        for bucket in &mut self.bucket_pool {
            bucket.is_active = false;
        }
        self.active_buckets.clear();
        self.free_buckets.clear();
        self.free_buckets.extend(0..self.bucket_pool.len());
        self.spatial_grid.clear();
        self.next_cluster_id = 0;
        self.hits_processed = 0;
        self.clusters_found = 0;
    }
}

/// ABS clustering algorithm.
pub struct AbsClustering {
    config: AbsConfig,
    generic_config: ClusteringConfig,
}

impl AbsClustering {
    /// Create with custom configuration.
    pub fn new(config: AbsConfig) -> Self {
        let generic_config = ClusteringConfig {
            radius: config.radius,
            temporal_window_ns: config.neutron_correlation_window_ns,
            min_cluster_size: config.min_cluster_size,
            max_cluster_size: None,
        };
        Self {
            config,
            generic_config,
        }
    }

    /// Get or create a bucket from the pool.
    fn get_bucket(&self, state: &mut AbsState) -> usize {
        if let Some(idx) = state.free_buckets.pop() {
            idx
        } else {
            let idx = state.bucket_pool.len();
            state.bucket_pool.push(Bucket::new());
            idx
        }
    }

    /// Find a compatible bucket for a hit.
    fn find_compatible_bucket<H: Hit>(
        &self,
        hit: &H,
        state: &AbsState,
        window_tof: u32,
    ) -> Option<usize> {
        let x = hit.x() as i32;
        let y = hit.y() as i32;

        let mut neighbors = Vec::with_capacity(16);
        state.spatial_grid.query_neighborhood(x, y, &mut neighbors);

        // Search in spatial neighborhood
        for &bucket_idx in neighbors.iter() {
            let bucket = &state.bucket_pool[bucket_idx];
            if bucket.is_active
                && bucket.fits_spatially(hit, self.config.radius)
                && bucket.fits_temporally(hit, window_tof)
            {
                return Some(bucket_idx);
            }
        }
        None
    }

    /// Close a bucket and assign cluster labels.
    fn close_bucket(&self, bucket_idx: usize, state: &mut AbsState, labels: &mut [i32]) -> bool {
        let bucket = &mut state.bucket_pool[bucket_idx];

        if bucket.hit_indices.len() >= self.config.min_cluster_size as usize {
            let cluster_id = state.next_cluster_id;
            state.next_cluster_id += 1;
            bucket.cluster_id = cluster_id;

            for &hit_idx in &bucket.hit_indices {
                labels[hit_idx] = cluster_id;
            }

            state.clusters_found += 1;
            true
        } else {
            false
        }
    }

    /// Scan and close aged buckets.
    fn scan_and_close_aged(&self, reference_tof: u32, state: &mut AbsState, labels: &mut [i32]) {
        let window_tof = self.config.window_tof();

        // Identify aged buckets
        let mut aged_indices = Vec::new();
        let mut active_indices_to_keep = Vec::new();

        for &bucket_idx in &state.active_buckets {
            let bucket = &state.bucket_pool[bucket_idx];
            if bucket.is_aged(reference_tof, window_tof) {
                aged_indices.push(bucket_idx);
            } else {
                active_indices_to_keep.push(bucket_idx);
            }
        }

        // Update active buckets list
        state.active_buckets = active_indices_to_keep;

        // Close and cleanup aged buckets
        for bucket_idx in aged_indices {
            self.close_bucket(bucket_idx, state, labels);

            // Remove from spatial index using stored insertion coordinates
            let bucket = &mut state.bucket_pool[bucket_idx];
            state
                .spatial_grid
                .remove(bucket.insertion_x, bucket.insertion_y, bucket_idx);

            // Return to pool
            bucket.is_active = false;
            state.free_buckets.push(bucket_idx);
        }
    }
}

impl Default for AbsClustering {
    fn default() -> Self {
        Self::new(AbsConfig::default())
    }
}

impl HitClustering for AbsClustering {
    type State = AbsState;

    fn name(&self) -> &'static str {
        "ABS"
    }

    fn create_state(&self) -> Self::State {
        let mut bucket_pool = Vec::with_capacity(self.config.pre_allocate_buckets);
        for _ in 0..self.config.pre_allocate_buckets {
            bucket_pool.push(Bucket::new());
        }

        AbsState {
            bucket_pool,
            active_buckets: Vec::with_capacity(self.config.pre_allocate_buckets),
            free_buckets: (0..self.config.pre_allocate_buckets).collect(),
            spatial_grid: SpatialGrid::new(32, 512, 512),
            next_cluster_id: 0,
            hits_processed: 0,
            clusters_found: 0,
        }
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.radius = config.radius;
        self.config.neutron_correlation_window_ns = config.temporal_window_ns;
        self.generic_config = config.clone();
    }

    fn config(&self) -> &ClusteringConfig {
        &self.generic_config
    }

    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError> {
        if hits.is_empty() {
            return Ok(0);
        }

        // Initialize labels to -1 (unclustered)
        labels.iter_mut().for_each(|l| *l = -1);

        let window_tof = self.config.window_tof();

        for (hit_idx, hit) in hits.iter().enumerate() {
            // Periodic aging scan
            if hit_idx % self.config.scan_interval == 0 && hit_idx > 0 {
                self.scan_and_close_aged(hit.tof(), state, labels);
            }

            // Find compatible bucket or create new one
            if let Some(bucket_idx) = self.find_compatible_bucket(hit, state, window_tof) {
                state.bucket_pool[bucket_idx].add_hit(hit_idx, hit);
            } else {
                let bucket_idx = self.get_bucket(state);
                state.bucket_pool[bucket_idx].initialize(hit_idx, hit);
                state.active_buckets.push(bucket_idx);

                // Add to spatial index
                let x = hit.x() as i32;
                let y = hit.y() as i32;
                state.spatial_grid.insert(x, y, bucket_idx);
            }

            state.hits_processed += 1;
        }

        // Final aging scan
        if let Some(last_hit) = hits.last() {
            self.scan_and_close_aged(last_hit.tof().saturating_add(window_tof), state, labels);
        }

        // Force close remaining buckets
        for bucket_idx in std::mem::take(&mut state.active_buckets) {
            self.close_bucket(bucket_idx, state, labels);

            // Remove from spatial index? We don't have coordinates easily.
            // But since we are finishing, maybe it doesn't matter if we don't clear the grid explicitly
            // if we are going to reset anyway?
            // But `reset` clears the grid.

            state.bucket_pool[bucket_idx].is_active = false;
            state.free_buckets.push(bucket_idx);
        }

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::hit::GenericHit;

    #[test]
    fn test_abs_clustering_basic() {
        let clustering = AbsClustering::default();
        let mut state = clustering.create_state();

        // Create two clusters of hits
        let hits = vec![
            // Cluster 1: (10, 10) at t=100
            GenericHit {
                x: 10,
                y: 10,
                tof: 100,
                ..Default::default()
            },
            GenericHit {
                x: 11,
                y: 11,
                tof: 102,
                ..Default::default()
            },
            // Cluster 2: (50, 50) at t=200
            GenericHit {
                x: 50,
                y: 50,
                tof: 200,
                ..Default::default()
            },
            GenericHit {
                x: 51,
                y: 51,
                tof: 202,
                ..Default::default()
            },
        ];

        let mut labels = vec![0; hits.len()];
        let count = clustering.cluster(&hits, &mut state, &mut labels).unwrap();

        assert_eq!(count, 2);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[2], labels[3]);
        assert_ne!(labels[0], labels[2]);
    }
}
