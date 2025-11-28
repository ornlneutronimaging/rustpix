//! Neutron event output type.
//!
//! See IMPLEMENTATION_PLAN.md Part 2.2 for detailed specification.

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
    pub _reserved: [u8; 3],
}

impl Neutron {
    /// Create a new neutron from cluster data.
    pub fn new(x: f64, y: f64, tof: u32, tot: u16, n_hits: u16, chip_id: u8) -> Self {
        Self {
            x,
            y,
            tof,
            tot,
            n_hits,
            chip_id,
            _reserved: [0; 3],
        }
    }

    /// TOF in nanoseconds.
    #[inline]
    pub fn tof_ns(&self) -> f64 {
        self.tof as f64 * 25.0
    }

    /// TOF in milliseconds.
    #[inline]
    pub fn tof_ms(&self) -> f64 {
        self.tof_ns() / 1_000_000.0
    }

    /// Pixel coordinates (divide by super-resolution factor).
    #[inline]
    pub fn pixel_coords(&self, super_res: f64) -> (f64, f64) {
        (self.x / super_res, self.y / super_res)
    }

    /// Cluster size category.
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
    Single,
    Small,
    Medium,
    Large,
}

/// Statistics for a collection of neutrons.
#[derive(Clone, Debug, Default)]
pub struct NeutronStatistics {
    pub count: usize,
    pub mean_tof: f64,
    pub std_tof: f64,
    pub mean_tot: f64,
    pub mean_cluster_size: f64,
    pub single_hit_fraction: f64,
    pub x_range: (f64, f64),
    pub y_range: (f64, f64),
    pub tof_range: (u32, u32),
}

impl NeutronStatistics {
    /// Calculate statistics from a slice of neutrons.
    pub fn from_neutrons(neutrons: &[Neutron]) -> Self {
        if neutrons.is_empty() {
            return Self::default();
        }

        let count = neutrons.len();
        let sum_tof: f64 = neutrons.iter().map(|n| n.tof as f64).sum();
        let mean_tof = sum_tof / count as f64;

        let variance: f64 = neutrons
            .iter()
            .map(|n| (n.tof as f64 - mean_tof).powi(2))
            .sum::<f64>()
            / count as f64;
        let std_tof = variance.sqrt();

        let mean_tot = neutrons.iter().map(|n| n.tot as f64).sum::<f64>() / count as f64;
        let mean_cluster_size =
            neutrons.iter().map(|n| n.n_hits as f64).sum::<f64>() / count as f64;
        let single_hit_fraction =
            neutrons.iter().filter(|n| n.n_hits == 1).count() as f64 / count as f64;

        let x_min = neutrons
            .iter()
            .map(|n| n.x)
            .fold(f64::INFINITY, f64::min);
        let x_max = neutrons
            .iter()
            .map(|n| n.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let y_min = neutrons
            .iter()
            .map(|n| n.y)
            .fold(f64::INFINITY, f64::min);
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
            mean_tot,
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
        assert_eq!(neutron.x, 1024.0);
        assert_eq!(neutron.y, 2048.0);
        assert_eq!(neutron.tof, 1000);
        assert_eq!(neutron.tot, 150);
        assert_eq!(neutron.n_hits, 5);
    }

    #[test]
    fn test_tof_conversions() {
        let neutron = Neutron::new(0.0, 0.0, 1000, 0, 1, 0);
        assert_eq!(neutron.tof_ns(), 25000.0);
        assert_eq!(neutron.tof_ms(), 0.025);
    }

    #[test]
    fn test_pixel_coords() {
        let neutron = Neutron::new(800.0, 1600.0, 0, 0, 1, 0);
        let (px, py) = neutron.pixel_coords(8.0);
        assert_eq!(px, 100.0);
        assert_eq!(py, 200.0);
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
