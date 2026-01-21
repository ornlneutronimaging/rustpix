//! Hit traits and types for pixel detector data.
//!

/// Core trait for all detector hit types.
///
/// A "hit" represents a single pixel activation event from a detector.
/// Different detector types (TPX3, TPX4, etc.) implement this trait.
pub trait Hit: Clone + Send + Sync {
    /// Time-of-flight in detector-native units (typically 25ns).
    fn tof(&self) -> u32;

    /// X coordinate in global detector space.
    fn x(&self) -> u16;

    /// Y coordinate in global detector space.
    fn y(&self) -> u16;

    /// Time-over-threshold (signal amplitude proxy).
    fn tot(&self) -> u16;

    /// Timestamp in detector-native units.
    fn timestamp(&self) -> u32;

    /// Chip identifier for multi-chip detectors.
    fn chip_id(&self) -> u8;

    /// TOF in nanoseconds.
    #[inline]
    fn tof_ns(&self) -> f64 {
        self.tof() as f64 * 25.0
    }

    /// Squared Euclidean distance to another hit.
    #[inline]
    fn distance_squared(&self, other: &impl Hit) -> f64 {
        let dx = self.x() as f64 - other.x() as f64;
        let dy = self.y() as f64 - other.y() as f64;
        dx * dx + dy * dy
    }

    /// Check if within spatial radius of another hit.
    #[inline]
    fn within_radius(&self, other: &impl Hit, radius: f64) -> bool {
        self.distance_squared(other) <= radius * radius
    }

    /// Check if within temporal window of another hit (in TOF units).
    #[inline]
    fn within_temporal_window(&self, other: &impl Hit, window_tof: u32) -> bool {
        let diff = if self.tof() > other.tof() {
            self.tof() - other.tof()
        } else {
            other.tof() - self.tof()
        };
        diff <= window_tof
    }
}

/// Hit with mutable cluster assignment.
pub trait ClusterableHit: Hit {
    /// Get current cluster ID (-1 = unassigned).
    fn cluster_id(&self) -> i32;

    /// Set cluster ID.
    fn set_cluster_id(&mut self, id: i32);
}

/// Generic hit type for detector-agnostic code.
///
/// Memory layout optimized for cache efficiency.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct GenericHit {
    /// Time-of-flight in 25ns units.
    pub tof: u32,
    /// Global X coordinate.
    pub x: u16,
    /// Global Y coordinate.
    pub y: u16,
    /// Timestamp in 25ns units.
    pub timestamp: u32,
    /// Time-over-threshold.
    pub tot: u16,
    /// Chip identifier.
    pub chip_id: u8,
    /// Padding for alignment.
    pub _padding: u8,
    /// Cluster assignment (-1 = unassigned).
    pub cluster_id: i32,
}

impl GenericHit {
    /// Create a new hit.
    pub fn new(tof: u32, x: u16, y: u16, timestamp: u32, tot: u16, chip_id: u8) -> Self {
        Self {
            tof,
            x,
            y,
            timestamp,
            tot,
            chip_id,
            _padding: 0,
            cluster_id: -1,
        }
    }
}

impl Hit for GenericHit {
    #[inline]
    fn tof(&self) -> u32 {
        self.tof
    }
    #[inline]
    fn x(&self) -> u16 {
        self.x
    }
    #[inline]
    fn y(&self) -> u16 {
        self.y
    }
    #[inline]
    fn tot(&self) -> u16 {
        self.tot
    }
    #[inline]
    fn timestamp(&self) -> u32 {
        self.timestamp
    }
    #[inline]
    fn chip_id(&self) -> u8 {
        self.chip_id
    }
}

impl ClusterableHit for GenericHit {
    #[inline]
    fn cluster_id(&self) -> i32 {
        self.cluster_id
    }
    #[inline]
    fn set_cluster_id(&mut self, id: i32) {
        self.cluster_id = id;
    }
}

impl Eq for GenericHit {}

impl Ord for GenericHit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tof.cmp(&other.tof)
    }
}

impl PartialOrd for GenericHit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_creation() {
        let hit = GenericHit::new(1000, 128, 256, 500, 50, 0);
        assert_eq!(hit.tof(), 1000);
        assert_eq!(hit.x(), 128);
        assert_eq!(hit.y(), 256);
        assert_eq!(hit.tot(), 50);
        assert_eq!(hit.chip_id(), 0);
        assert_eq!(hit.cluster_id(), -1);
    }

    #[test]
    fn test_tof_ns_conversion() {
        let hit = GenericHit::new(1000, 0, 0, 0, 0, 0);
        assert_eq!(hit.tof_ns(), 25000.0);
    }

    #[test]
    fn test_distance_squared() {
        let hit1 = GenericHit::new(0, 0, 0, 0, 0, 0);
        let hit2 = GenericHit::new(0, 3, 4, 0, 0, 0);
        assert_eq!(hit1.distance_squared(&hit2), 25.0);
    }

    #[test]
    fn test_within_radius() {
        let hit1 = GenericHit::new(0, 0, 0, 0, 0, 0);
        let hit2 = GenericHit::new(0, 3, 4, 0, 0, 0);
        assert!(hit1.within_radius(&hit2, 5.0));
        assert!(!hit1.within_radius(&hit2, 4.9));
    }

    #[test]
    fn test_within_temporal_window() {
        let hit1 = GenericHit::new(1000, 0, 0, 0, 0, 0);
        let hit2 = GenericHit::new(1003, 0, 0, 0, 0, 0);
        assert!(hit1.within_temporal_window(&hit2, 3));
        assert!(!hit1.within_temporal_window(&hit2, 2));
    }
}
