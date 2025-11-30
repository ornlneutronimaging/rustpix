//! TPX3-specific hit type.
//!
//! See IMPLEMENTATION_PLAN.md Part 3.2 for detailed specification.

use rustpix_core::hit::{ClusterableHit, Hit};

/// TPX3 hit with optimized memory layout.
///
/// Size: 20 bytes (packed)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Tpx3Hit {
    /// Time-of-flight in 25ns units.
    pub tof: u32,
    /// Global X coordinate.
    pub x: u16,
    /// Global Y coordinate.
    pub y: u16,
    /// Timestamp in 25ns units.
    pub timestamp: u32,
    /// Time-over-threshold (10-bit, stored as u16).
    pub tot: u16,
    /// Chip identifier (0-3 for quad arrangement).
    pub chip_id: u8,
    /// Padding for alignment.
    pub _padding: u8,
    /// Cluster assignment (-1 = unassigned).
    pub cluster_id: i32,
}

impl Tpx3Hit {
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

impl From<(u32, u16, u16, u32, u16, u8)> for Tpx3Hit {
    fn from(tuple: (u32, u16, u16, u32, u16, u8)) -> Self {
        Self::new(tuple.0, tuple.1, tuple.2, tuple.3, tuple.4, tuple.5)
    }
}

impl Hit for Tpx3Hit {
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

impl ClusterableHit for Tpx3Hit {
    #[inline]
    fn cluster_id(&self) -> i32 {
        self.cluster_id
    }
    #[inline]
    fn set_cluster_id(&mut self, id: i32) {
        self.cluster_id = id;
    }
}

impl Eq for Tpx3Hit {}

impl Ord for Tpx3Hit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tof.cmp(&other.tof)
    }
}

impl PartialOrd for Tpx3Hit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Timestamp rollover correction.
///
/// TPX3 uses 30-bit timestamps that can roll over. This function
/// corrects the hit timestamp relative to the TDC timestamp.
///
/// Formula: if hit_ts + 0x400000 < tdc_ts, extend by 0x40000000
#[inline]
pub fn correct_timestamp_rollover(hit_timestamp: u32, tdc_timestamp: u32) -> u32 {
    const EXTENSION_THRESHOLD: u32 = 0x400000;
    const EXTENSION_VALUE: u32 = 0x40000000;

    if hit_timestamp.wrapping_add(EXTENSION_THRESHOLD) < tdc_timestamp {
        hit_timestamp.wrapping_add(EXTENSION_VALUE)
    } else {
        hit_timestamp
    }
}

/// Calculate TOF with TDC correction.
///
/// If the raw TOF exceeds the TDC period, subtract one period.
///
/// # Arguments
///
/// * `timestamp` - Hit timestamp in 25ns units.
/// * `tdc_timestamp` - TDC timestamp in 25ns units.
/// * `tdc_correction_25ns` - The TDC period in 25ns units (1 / TDC_frequency).
///   For SNS (60Hz), this is approximately 666,667 units (16.67ms / 25ns).
#[inline]
pub fn calculate_tof(timestamp: u32, tdc_timestamp: u32, tdc_correction_25ns: u32) -> u32 {
    let raw_tof = timestamp.wrapping_sub(tdc_timestamp);
    if raw_tof > tdc_correction_25ns {
        raw_tof.wrapping_sub(tdc_correction_25ns)
    } else {
        raw_tof
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_creation() {
        let hit = Tpx3Hit::new(1000, 128, 256, 500, 50, 0);
        assert_eq!(hit.tof(), 1000);
        assert_eq!(hit.x(), 128);
        assert_eq!(hit.y(), 256);
        assert_eq!(hit.tot(), 50);
        assert_eq!(hit.chip_id(), 0);
        assert_eq!(hit.cluster_id(), -1);
    }

    #[test]
    fn test_hit_trait() {
        let hit = Tpx3Hit::new(1000, 100, 200, 500, 50, 0);
        assert_eq!(hit.tof_ns(), 25000.0);
    }

    #[test]
    fn test_timestamp_rollover() {
        // Normal case - no correction needed
        assert_eq!(correct_timestamp_rollover(100, 50), 100);

        // Rollover case - extension needed
        let hit_ts = 0x100;
        let tdc_ts = 0x500000;
        let corrected = correct_timestamp_rollover(hit_ts, tdc_ts);
        assert_eq!(corrected, hit_ts + 0x40000000);
    }

    #[test]
    fn test_tof_calculation() {
        let timestamp = 1000u32;
        let tdc_timestamp = 500u32;
        let correction = 666667u32; // ~1/60Hz in 25ns units

        let tof = calculate_tof(timestamp, tdc_timestamp, correction);
        assert_eq!(tof, 500);
    }
}
