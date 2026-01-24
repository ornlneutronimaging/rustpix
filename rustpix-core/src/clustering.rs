//! Clustering algorithm traits and configuration.
//!

// Re-export ClusteringError for convenience
pub use crate::error::ClusteringError;

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
    #[must_use]
    pub fn venus_defaults() -> Self {
        Self::default()
    }

    /// Temporal window in TOF units (25ns).
    #[inline]
    #[must_use]
    pub fn window_tof(&self) -> u32 {
        let window = (self.temporal_window_ns / 25.0).ceil();
        if window <= 0.0 {
            return 0;
        }
        if window >= f64::from(u32::MAX) {
            return u32::MAX;
        }
        format!("{window:.0}").parse::<u32>().unwrap_or(u32::MAX)
    }

    /// Set spatial radius.
    #[must_use]
    pub fn with_radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }

    /// Set temporal window.
    #[must_use]
    pub fn with_temporal_window_ns(mut self, window_ns: f64) -> Self {
        self.temporal_window_ns = window_ns;
        self
    }

    /// Set minimum cluster size.
    #[must_use]
    pub fn with_min_cluster_size(mut self, size: u16) -> Self {
        self.min_cluster_size = size;
        self
    }

    /// Set maximum cluster size.
    #[must_use]
    pub fn with_max_cluster_size(mut self, size: u16) -> Self {
        self.max_cluster_size = Some(size);
        self
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = ClusteringConfig::default();
        assert!((config.radius - 5.0).abs() < f64::EPSILON);
        assert!((config.temporal_window_ns - 75.0).abs() < f64::EPSILON);
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

        assert!((config.radius - 10.0).abs() < f64::EPSILON);
        assert!((config.temporal_window_ns - 100.0).abs() < f64::EPSILON);
        assert_eq!(config.min_cluster_size, 2);
        assert_eq!(config.max_cluster_size, Some(100));
    }
}
