//! Neutron data types and traits.

use crate::{Centroid, TimeOfArrival};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Core data structure for a detected neutron event.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NeutronData {
    /// Centroid X coordinate (sub-pixel precision).
    pub x: f64,
    /// Centroid Y coordinate (sub-pixel precision).
    pub y: f64,
    /// Time of arrival (average or weighted).
    pub toa: TimeOfArrival,
    /// Total time over threshold (sum of all hits).
    pub tot_sum: u32,
    /// Number of hits in the cluster.
    pub cluster_size: u16,
}

impl NeutronData {
    /// Creates a new neutron data structure.
    pub fn new(x: f64, y: f64, toa: u64, tot_sum: u32, cluster_size: u16) -> Self {
        Self {
            x,
            y,
            toa: TimeOfArrival::new(toa),
            tot_sum,
            cluster_size,
        }
    }

    /// Creates a neutron from a centroid.
    pub fn from_centroid(centroid: Centroid) -> Self {
        Self {
            x: centroid.x,
            y: centroid.y,
            toa: centroid.toa,
            tot_sum: centroid.tot_sum,
            cluster_size: centroid.cluster_size,
        }
    }
}

/// Trait for neutron event data.
///
/// This trait provides a common interface for neutron data
/// that has been extracted from clustered hits.
pub trait Neutron: Send + Sync {
    /// Returns the centroid X coordinate.
    fn x(&self) -> f64;

    /// Returns the centroid Y coordinate.
    fn y(&self) -> f64;

    /// Returns the time of arrival.
    fn toa(&self) -> TimeOfArrival;

    /// Returns the total time over threshold.
    fn tot_sum(&self) -> u32;

    /// Returns the cluster size.
    fn cluster_size(&self) -> u16;
}

impl Neutron for NeutronData {
    #[inline]
    fn x(&self) -> f64 {
        self.x
    }

    #[inline]
    fn y(&self) -> f64 {
        self.y
    }

    #[inline]
    fn toa(&self) -> TimeOfArrival {
        self.toa
    }

    #[inline]
    fn tot_sum(&self) -> u32 {
        self.tot_sum
    }

    #[inline]
    fn cluster_size(&self) -> u16 {
        self.cluster_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neutron_data() {
        let neutron = NeutronData::new(10.5, 20.3, 1000, 150, 5);
        assert!((neutron.x() - 10.5).abs() < f64::EPSILON);
        assert!((neutron.y() - 20.3).abs() < f64::EPSILON);
        assert_eq!(neutron.toa().as_u64(), 1000);
        assert_eq!(neutron.tot_sum(), 150);
        assert_eq!(neutron.cluster_size(), 5);
    }
}
