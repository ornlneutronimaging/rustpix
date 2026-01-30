//! Statistics tracking for load and processing operations.

use std::time::Duration;

/// Statistics for the current session.
#[derive(Default)]
pub struct Statistics {
    /// Number of hits loaded.
    pub hit_count: usize,
    /// Time taken to load the file.
    pub load_duration: Option<Duration>,
    /// TOF range maximum (in 25ns units).
    pub tof_max: u32,
    /// Number of neutrons after clustering.
    pub neutron_count: usize,
    /// Time taken to cluster.
    pub cluster_duration: Option<Duration>,
    /// Average cluster size (hits per neutron).
    pub avg_cluster_size: f64,
}

impl Statistics {
    /// Clear all statistics.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Calculate processing speed in hits/sec.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn load_speed(&self) -> Option<f64> {
        self.load_duration.map(|d| {
            let secs = d.as_secs_f64();
            if secs > 0.0 {
                self.hit_count as f64 / secs
            } else {
                0.0
            }
        })
    }

    /// Calculate clustering speed in neutrons/sec.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn cluster_speed(&self) -> Option<f64> {
        self.cluster_duration.map(|d| {
            let secs = d.as_secs_f64();
            if secs > 0.0 {
                self.neutron_count as f64 / secs
            } else {
                0.0
            }
        })
    }

    /// Convert TOF range to milliseconds given TDC frequency.
    #[must_use]
    #[allow(clippy::similar_names)]
    pub fn tof_range_ms(&self, tdc_frequency: f64) -> f64 {
        let tdc_period_s = 1.0 / tdc_frequency;
        let tdc_period_ms = tdc_period_s * 1000.0;
        let max_units = f64::from(self.tof_max);
        if max_units > 0.0 {
            tdc_period_ms
        } else {
            0.0
        }
    }
}
