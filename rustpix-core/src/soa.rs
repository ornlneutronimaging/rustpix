//! Structure of Arrays (`SoA`) types for efficient processing.
//!
//! This module defines the `HitBatch` structure which stores hit data
//! in parallel vectors (`SoA` layout) rather than an array of structs (`AoS`).
//! This layout works better with modern CPU caches and SIMD instructions.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A batch of hits stored in Structure of Arrays (`SoA`) format.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HitBatch {
    /// Columnar storage for X coordinates.
    pub x: Vec<u16>,
    /// Columnar storage for Y coordinates.
    pub y: Vec<u16>,
    /// Columnar storage for Time-of-Flight (corrected).
    pub tof: Vec<u32>,
    /// Columnar storage for Time-over-Threshold.
    pub tot: Vec<u16>,
    /// Columnar storage for global timestamps.
    pub timestamp: Vec<u32>,
    /// Columnar storage for Chip IDs (optional if batch is per-chip).
    pub chip_id: Vec<u8>,
    /// Cluster assignments (output of clustering).
    pub cluster_id: Vec<i32>,
}

impl HitBatch {
    /// Creates a new empty batch with specified capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            x: Vec::with_capacity(capacity),
            y: Vec::with_capacity(capacity),
            tof: Vec::with_capacity(capacity),
            tot: Vec::with_capacity(capacity),
            timestamp: Vec::with_capacity(capacity),
            chip_id: Vec::with_capacity(capacity),
            cluster_id: Vec::with_capacity(capacity),
        }
    }

    /// Returns the number of hits in the batch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Returns true if the batch is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Clears all vectors in the batch.
    pub fn clear(&mut self) {
        self.x.clear();
        self.y.clear();
        self.tof.clear();
        self.tot.clear();
        self.timestamp.clear();
        self.chip_id.clear();
        self.cluster_id.clear();
    }

    /// Appends all hits from another batch to this one.
    pub fn append(&mut self, other: &HitBatch) {
        self.x.extend_from_slice(&other.x);
        self.y.extend_from_slice(&other.y);
        self.tof.extend_from_slice(&other.tof);
        self.tot.extend_from_slice(&other.tot);
        self.timestamp.extend_from_slice(&other.timestamp);
        self.chip_id.extend_from_slice(&other.chip_id);
        self.cluster_id.extend_from_slice(&other.cluster_id);
    }

    /// Pushes a single hit into the batch.
    #[allow(clippy::too_many_arguments)]
    pub fn push(&mut self, x: u16, y: u16, tof: u32, tot: u16, timestamp: u32, chip_id: u8) {
        self.x.push(x);
        self.y.push(y);
        self.tof.push(tof);
        self.tot.push(tot);
        self.timestamp.push(timestamp);
        self.chip_id.push(chip_id);
        self.cluster_id.push(-1); // Default unclustered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_batch_operations() {
        let mut batch = HitBatch::with_capacity(10);
        assert!(batch.is_empty());

        batch.push(10, 20, 1000, 5, 123_456, 0);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.x[0], 10);
        assert_eq!(batch.cluster_id[0], -1);

        batch.push(11, 21, 1001, 6, 123_457, 0);
        assert_eq!(batch.len(), 2);

        batch.clear();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }
}
