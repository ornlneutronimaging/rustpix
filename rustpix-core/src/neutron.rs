//! Neutron event output type.
//!

/// A detected neutron event after clustering and centroid extraction.
///
/// Coordinates are in super-resolution space (default 8x pixel resolution).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Neutron {
    /// X coordinate in super-resolution space.
    pub x: f64,
    /// Y coordinate in super-resolution space.
    pub y: f64,
    /// Time-of-flight in 25ns units.
    pub tof: u32,
    /// Combined time-over-threshold.
    pub tot: u16,
    /// Number of hits in cluster.
    pub n_hits: u16,
    /// Source chip ID.
    pub chip_id: u8,
    /// Reserved for alignment.
    #[doc(hidden)]
    pub reserved: [u8; 3],
}

impl Neutron {
    /// Create a new neutron from cluster data.
    #[must_use]
    pub fn new(x: f64, y: f64, tof: u32, tot: u16, n_hits: u16, chip_id: u8) -> Self {
        Self {
            x,
            y,
            tof,
            tot,
            n_hits,
            chip_id,
            reserved: [0; 3],
        }
    }

    /// TOF in nanoseconds.
    #[inline]
    #[must_use]
    pub fn tof_ns(&self) -> f64 {
        f64::from(self.tof) * 25.0
    }

    /// TOF in milliseconds.
    #[inline]
    #[must_use]
    pub fn tof_ms(&self) -> f64 {
        self.tof_ns() / 1_000_000.0
    }

    /// Pixel coordinates (divide by super-resolution factor).
    #[inline]
    #[must_use]
    pub fn pixel_coords(&self, super_res: f64) -> (f64, f64) {
        (self.x / super_res, self.y / super_res)
    }

    /// Cluster size category.
    #[must_use]
    pub fn cluster_size_category(&self) -> ClusterSize {
        match self.n_hits {
            1 => ClusterSize::Single,
            2..=4 => ClusterSize::Small,
            5..=10 => ClusterSize::Medium,
            _ => ClusterSize::Large,
        }
    }
}

/// Cluster size categories for analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterSize {
    /// Single-hit cluster.
    Single,
    /// Small cluster (2-4 hits).
    Small,
    /// Medium cluster (5-10 hits).
    Medium,
    /// Large cluster (>10 hits).
    Large,
}

/// Statistics for a collection of neutrons.
#[derive(Clone, Debug, Default)]
pub struct NeutronStatistics {
    /// Number of neutrons in the sample.
    pub count: usize,
    /// Mean time-of-flight (25ns units).
    pub mean_tof: f64,
    /// Standard deviation of time-of-flight (25ns units).
    pub std_tof: f64,
    /// Mean time-over-threshold.
    pub mean_tot: f64,
    /// Mean cluster size (hits per neutron).
    pub mean_cluster_size: f64,
    /// Fraction of single-hit neutrons.
    pub single_hit_fraction: f64,
    /// Min/max X coordinate.
    pub x_range: (f64, f64),
    /// Min/max Y coordinate.
    pub y_range: (f64, f64),
    /// Min/max time-of-flight (25ns units).
    pub tof_range: (u32, u32),
}

/// Structure-of-arrays neutron output.
#[derive(Clone, Debug, Default)]
pub struct NeutronBatch {
    /// X coordinates (super-resolution space).
    pub x: Vec<f64>,
    /// Y coordinates (super-resolution space).
    pub y: Vec<f64>,
    /// Time-of-flight values (25ns units).
    pub tof: Vec<u32>,
    /// Time-over-threshold values.
    pub tot: Vec<u16>,
    /// Number of hits per neutron.
    pub n_hits: Vec<u16>,
    /// Chip ID per neutron.
    pub chip_id: Vec<u8>,
}

impl NeutronBatch {
    /// Create a batch with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            x: Vec::with_capacity(capacity),
            y: Vec::with_capacity(capacity),
            tof: Vec::with_capacity(capacity),
            tot: Vec::with_capacity(capacity),
            n_hits: Vec::with_capacity(capacity),
            chip_id: Vec::with_capacity(capacity),
        }
    }

    /// Number of neutrons in the batch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Returns true when the batch is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Append a single neutron to the batch.
    pub fn push(&mut self, neutron: Neutron) {
        self.x.push(neutron.x);
        self.y.push(neutron.y);
        self.tof.push(neutron.tof);
        self.tot.push(neutron.tot);
        self.n_hits.push(neutron.n_hits);
        self.chip_id.push(neutron.chip_id);
    }

    /// Append all neutrons from another batch.
    pub fn append(&mut self, other: &NeutronBatch) {
        self.x.extend_from_slice(&other.x);
        self.y.extend_from_slice(&other.y);
        self.tof.extend_from_slice(&other.tof);
        self.tot.extend_from_slice(&other.tot);
        self.n_hits.extend_from_slice(&other.n_hits);
        self.chip_id.extend_from_slice(&other.chip_id);
    }

    /// Clear all neutron data from the batch.
    pub fn clear(&mut self) {
        self.x.clear();
        self.y.clear();
        self.tof.clear();
        self.tot.clear();
        self.n_hits.clear();
        self.chip_id.clear();
    }
}

impl NeutronStatistics {
    /// Calculate statistics from a slice of neutrons.
    pub fn from_neutrons(neutrons: &[Neutron]) -> Self {
        if neutrons.is_empty() {
            return Self::default();
        }

        let count = neutrons.len();
        let count_u32_value = u32::try_from(count).unwrap_or(u32::MAX);
        let count_as_f64 = f64::from(count_u32_value);
        let sum_tof: f64 = neutrons.iter().map(|n| f64::from(n.tof)).sum();
        let mean_tof = sum_tof / count_as_f64;

        let variance: f64 = neutrons
            .iter()
            .map(|n| (f64::from(n.tof) - mean_tof).powi(2))
            .sum::<f64>()
            / count_as_f64;
        let std_tof = variance.sqrt();

        let mean_total_tot = neutrons.iter().map(|n| f64::from(n.tot)).sum::<f64>() / count_as_f64;
        let mean_cluster_size =
            neutrons.iter().map(|n| f64::from(n.n_hits)).sum::<f64>() / count_as_f64;
        let single_hit_count =
            u32::try_from(neutrons.iter().filter(|n| n.n_hits == 1).count()).unwrap_or(u32::MAX);
        let single_hit_fraction = f64::from(single_hit_count) / count_as_f64;

        let x_min = neutrons.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
        let x_max = neutrons
            .iter()
            .map(|n| n.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let y_min = neutrons.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
        let y_max = neutrons
            .iter()
            .map(|n| n.y)
            .fold(f64::NEG_INFINITY, f64::max);
        let tof_min = neutrons.iter().map(|n| n.tof).min().unwrap_or(0);
        let tof_max = neutrons.iter().map(|n| n.tof).max().unwrap_or(0);

        Self {
            count,
            mean_tof,
            std_tof,
            mean_tot: mean_total_tot,
            mean_cluster_size,
            single_hit_fraction,
            x_range: (x_min, x_max),
            y_range: (y_min, y_max),
            tof_range: (tof_min, tof_max),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neutron_creation() {
        let neutron = Neutron::new(1024.0, 2048.0, 1000, 150, 5, 0);
        assert!((neutron.x - 1024.0).abs() < f64::EPSILON);
        assert!((neutron.y - 2048.0).abs() < f64::EPSILON);
        assert_eq!(neutron.tof, 1000);
        assert_eq!(neutron.tot, 150);
        assert_eq!(neutron.n_hits, 5);
    }

    #[test]
    fn test_tof_conversions() {
        let neutron = Neutron::new(0.0, 0.0, 1000, 0, 1, 0);
        assert!((neutron.tof_ns() - 25_000.0).abs() < f64::EPSILON);
        assert!((neutron.tof_ms() - 0.025).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pixel_coords() {
        let neutron = Neutron::new(800.0, 1600.0, 0, 0, 1, 0);
        let (px, py) = neutron.pixel_coords(8.0);
        assert!((px - 100.0).abs() < f64::EPSILON);
        assert!((py - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cluster_size_category() {
        assert_eq!(
            Neutron::new(0.0, 0.0, 0, 0, 1, 0).cluster_size_category(),
            ClusterSize::Single
        );
        assert_eq!(
            Neutron::new(0.0, 0.0, 0, 0, 3, 0).cluster_size_category(),
            ClusterSize::Small
        );
        assert_eq!(
            Neutron::new(0.0, 0.0, 0, 0, 7, 0).cluster_size_category(),
            ClusterSize::Medium
        );
        assert_eq!(
            Neutron::new(0.0, 0.0, 0, 0, 15, 0).cluster_size_category(),
            ClusterSize::Large
        );
    }

    #[test]
    fn test_statistics() {
        let neutrons = vec![
            Neutron::new(100.0, 200.0, 1000, 50, 1, 0),
            Neutron::new(110.0, 210.0, 1010, 60, 3, 0),
            Neutron::new(105.0, 205.0, 1005, 55, 2, 0),
        ];
        let stats = NeutronStatistics::from_neutrons(&neutrons);
        assert_eq!(stats.count, 3);
        assert!((stats.mean_tof - 1005.0).abs() < 0.01);
        assert!((stats.single_hit_fraction - 1.0 / 3.0).abs() < 0.01);
    }
}
