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

/// ABS clustering state.
///
/// TODO: Implement bucket pool and spatial index (see IMPLEMENTATION_PLAN.md Part 4.1)
pub struct AbsState {
    next_cluster_id: i32,
    hits_processed: usize,
    clusters_found: usize,
}

impl Default for AbsState {
    fn default() -> Self {
        Self {
            next_cluster_id: 0,
            hits_processed: 0,
            clusters_found: 0,
        }
    }
}

impl ClusteringState for AbsState {
    fn reset(&mut self) {
        self.next_cluster_id = 0;
        self.hits_processed = 0;
        self.clusters_found = 0;
    }
}

/// ABS clustering algorithm.
///
/// TODO: Full implementation in IMPLEMENTATION_PLAN.md Part 4.1
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
        AbsState::default()
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
        // TODO: Implement the actual ABS algorithm
        // See IMPLEMENTATION_PLAN.md Part 4.1 for full implementation
        //
        // Algorithm outline:
        // 1. For each hit:
        //    a. Scan and close aged buckets (every scan_interval hits)
        //    b. Find compatible bucket (spatial + temporal constraints)
        //    c. If found: add to bucket
        //    d. Else: create new bucket
        // 2. Final scan to close remaining buckets
        // 3. Assign cluster labels

        if hits.is_empty() {
            return Ok(0);
        }

        // Placeholder: assign each hit to its own cluster
        for (i, label) in labels.iter_mut().enumerate() {
            *label = i as i32;
        }

        state.hits_processed = hits.len();
        state.clusters_found = hits.len();

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

    #[test]
    fn test_abs_config_defaults() {
        let config = AbsConfig::default();
        assert_eq!(config.radius, 5.0);
        assert_eq!(config.neutron_correlation_window_ns, 75.0);
        assert_eq!(config.window_tof(), 3);
    }

    #[test]
    fn test_abs_state_reset() {
        let mut state = AbsState::default();
        state.hits_processed = 100;
        state.clusters_found = 10;
        state.reset();
        assert_eq!(state.hits_processed, 0);
        assert_eq!(state.clusters_found, 0);
    }
}
