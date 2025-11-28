//! Clustering algorithm traits and configuration.
//!
//! See IMPLEMENTATION_PLAN.md Part 2.3 for detailed specification.

// Re-export ClusteringError for convenience
pub use crate::error::ClusteringError;
use crate::hit::Hit;

/// Configuration for clustering algorithms.
///
/// This is a generic configuration that all clustering algorithms accept.
/// Algorithm-specific configurations extend this.
#[derive(Clone, Debug)]
pub struct ClusteringConfig {
    /// Spatial radius for neighbor detection (pixels).
    pub radius: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
    /// Maximum cluster size (None = unlimited).
    pub max_cluster_size: Option<u16>,
}

impl Default for ClusteringConfig {
    fn default() -> Self {
        Self {
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            max_cluster_size: None,
        }
    }
}

impl ClusteringConfig {
    /// Create VENUS/SNS default configuration.
    pub fn venus_defaults() -> Self {
        Self::default()
    }

    /// Temporal window in TOF units (25ns).
    #[inline]
    pub fn window_tof(&self) -> u32 {
        (self.temporal_window_ns / 25.0).ceil() as u32
    }

    /// Set spatial radius.
    pub fn with_radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }

    /// Set temporal window.
    pub fn with_temporal_window_ns(mut self, window_ns: f64) -> Self {
        self.temporal_window_ns = window_ns;
        self
    }

    /// Set minimum cluster size.
    pub fn with_min_cluster_size(mut self, size: u16) -> Self {
        self.min_cluster_size = size;
        self
    }

    /// Set maximum cluster size.
    pub fn with_max_cluster_size(mut self, size: u16) -> Self {
        self.max_cluster_size = Some(size);
        self
    }
}

/// Base trait for clustering algorithm state.
///
/// State is separated from algorithm to enable thread-safe parallel processing.
/// Each thread gets its own state instance.
pub trait ClusteringState: Send {
    /// Reset state for reuse.
    fn reset(&mut self);
}

/// Statistics from a clustering operation.
#[derive(Clone, Debug, Default)]
pub struct ClusteringStatistics {
    pub hits_processed: usize,
    pub clusters_found: usize,
    pub noise_hits: usize,
    pub largest_cluster_size: usize,
    pub mean_cluster_size: f64,
    pub processing_time_us: u64,
}

/// Main trait for hit clustering algorithms.
///
/// Design principles (see IMPLEMENTATION_PLAN.md Part 2.3):
/// - **Stateless methods**: All mutable state passed via `ClusteringState`
/// - **Generic over hit type**: Works with any `Hit` implementation
/// - **Thread-safe**: Can be used from multiple threads with separate states
///
/// # Example
/// ```ignore
/// let clustering = AbsClustering::new(AbsConfig::default());
/// let mut state = clustering.create_state();
/// let mut labels = vec![-1i32; hits.len()];
/// let num_clusters = clustering.cluster(&hits, &mut state, &mut labels)?;
/// ```
pub trait HitClustering: Send + Sync {
    /// The state type used by this algorithm.
    type State: ClusteringState;

    /// Algorithm name for logging/debugging.
    fn name(&self) -> &'static str;

    /// Create a new state instance for this algorithm.
    fn create_state(&self) -> Self::State;

    /// Configure the algorithm.
    fn configure(&mut self, config: &ClusteringConfig);

    /// Get current configuration.
    fn config(&self) -> &ClusteringConfig;

    /// Cluster a batch of hits.
    ///
    /// # Arguments
    /// * `hits` - Slice of hits to cluster (should be sorted by TOF)
    /// * `state` - Mutable algorithm state
    /// * `labels` - Output cluster labels (-1 = noise/unclustered)
    ///
    /// # Returns
    /// Number of clusters found.
    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError>;

    /// Get statistics from the last clustering operation.
    fn statistics(&self, state: &Self::State) -> ClusteringStatistics;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = ClusteringConfig::default();
        assert_eq!(config.radius, 5.0);
        assert_eq!(config.temporal_window_ns, 75.0);
        assert_eq!(config.min_cluster_size, 1);
        assert_eq!(config.max_cluster_size, None);
    }

    #[test]
    fn test_window_tof_conversion() {
        let config = ClusteringConfig::default();
        // 75ns / 25ns = 3
        assert_eq!(config.window_tof(), 3);
    }

    #[test]
    fn test_config_builder() {
        let config = ClusteringConfig::default()
            .with_radius(10.0)
            .with_temporal_window_ns(100.0)
            .with_min_cluster_size(2)
            .with_max_cluster_size(100);

        assert_eq!(config.radius, 10.0);
        assert_eq!(config.temporal_window_ns, 100.0);
        assert_eq!(config.min_cluster_size, 2);
        assert_eq!(config.max_cluster_size, Some(100));
    }
}
