//! Histogram data structures for 3D hyperstack visualization.
//!
//! This module provides the `Hyperstack3D` structure which stores
//! binned event data in a 3D array indexed by `[tof, y, x]`.

// Allow unused methods/fields - they are reserved for Issue #82 (TOF slicer UI)
#![allow(dead_code)]

use std::ops::Range;

use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;

/// A 3D histogram storing counts indexed by (TOF bin, y, x).
///
/// Data is stored in row-major order: `data[tof * height * width + y * width + x]`
///
/// # Memory Layout
///
/// For a 200-bin × 512 × 512 hyperstack, memory usage is approximately 419 MB.
#[derive(Debug, Clone)]
pub struct Hyperstack3D {
    /// Flattened 3D data array.
    data: Vec<u64>,

    /// Number of TOF bins.
    n_tof_bins: usize,

    /// Width in pixels (X dimension).
    width: usize,

    /// Height in pixels (Y dimension).
    height: usize,

    /// Maximum TOF value in 25ns units.
    tof_max: u32,

    /// Width of each TOF bin in 25ns units.
    bin_width: f64,
}

impl Hyperstack3D {
    /// Create an empty hyperstack with the given dimensions.
    ///
    /// # Arguments
    ///
    /// * `n_tof_bins` - Number of TOF bins
    /// * `width` - Width in pixels (X)
    /// * `height` - Height in pixels (Y)
    /// * `tof_max` - Maximum TOF value in 25ns units (from TDC correction)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn new(n_tof_bins: usize, width: usize, height: usize, tof_max: u32) -> Self {
        let bin_width = if n_tof_bins > 0 {
            f64::from(tof_max) / n_tof_bins as f64
        } else {
            1.0
        };

        Self {
            data: vec![0u64; n_tof_bins * height * width],
            n_tof_bins,
            width,
            height,
            tof_max,
            bin_width,
        }
    }

    /// Build a hyperstack from a `HitBatch`.
    ///
    /// # Arguments
    ///
    /// * `batch` - The hit batch containing event data
    /// * `n_tof_bins` - Number of TOF bins to create
    /// * `tof_max` - Maximum TOF value in 25ns units
    /// * `width` - Width in pixels (typically 512)
    /// * `height` - Height in pixels (typically 512)
    #[must_use]
    pub fn from_hits(
        batch: &HitBatch,
        n_tof_bins: usize,
        tof_max: u32,
        width: usize,
        height: usize,
    ) -> Self {
        let mut hyperstack = Self::new(n_tof_bins, width, height, tof_max);

        for i in 0..batch.len() {
            let x = usize::from(batch.x[i]);
            let y = usize::from(batch.y[i]);
            let tof = batch.tof[i];

            // Calculate TOF bin
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let tof_bin = if hyperstack.bin_width > 0.0 {
                let bin = (f64::from(tof) / hyperstack.bin_width) as usize;
                bin.min(n_tof_bins.saturating_sub(1))
            } else {
                0
            };

            // Bounds check and increment
            if x < width && y < height && tof_bin < n_tof_bins {
                let idx = tof_bin * height * width + y * width + x;
                hyperstack.data[idx] += 1;
            }
        }

        hyperstack
    }

    /// Build a hyperstack from a `NeutronBatch`.
    ///
    /// Neutron positions are floats (super-resolution), so they are rounded
    /// to the nearest integer pixel coordinate.
    ///
    /// # Arguments
    ///
    /// * `batch` - The neutron batch containing event data
    /// * `n_tof_bins` - Number of TOF bins to create
    /// * `tof_max` - Maximum TOF value in 25ns units
    /// * `width` - Width in pixels (typically 512)
    /// * `height` - Height in pixels (typically 512)
    /// * `super_resolution_factor` - Super-resolution factor for neutron coordinates
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn from_neutrons(
        batch: &NeutronBatch,
        n_tof_bins: usize,
        tof_max: u32,
        width: usize,
        height: usize,
        super_resolution_factor: f64,
    ) -> Self {
        let mut hyperstack = Self::new(n_tof_bins, width, height, tof_max);
        let factor = if super_resolution_factor > 0.0 {
            super_resolution_factor
        } else {
            1.0
        };

        for i in 0..batch.len() {
            // Round float coordinates to nearest integer
            let x = (batch.x[i] / factor).round();
            let y = (batch.y[i] / factor).round();
            let tof = batch.tof[i];

            // Skip out-of-bounds
            if x < 0.0 || y < 0.0 {
                continue;
            }
            let x = x as usize;
            let y = y as usize;

            // Calculate TOF bin
            let tof_bin = if hyperstack.bin_width > 0.0 {
                let bin = (f64::from(tof) / hyperstack.bin_width) as usize;
                bin.min(n_tof_bins.saturating_sub(1))
            } else {
                0
            };

            // Bounds check and increment
            if x < width && y < height && tof_bin < n_tof_bins {
                let idx = tof_bin * height * width + y * width + x;
                hyperstack.data[idx] += 1;
            }
        }

        hyperstack
    }

    /// Get the count at a specific position.
    #[must_use]
    #[inline]
    pub fn get(&self, tof_bin: usize, y: usize, x: usize) -> Option<u64> {
        if tof_bin < self.n_tof_bins && y < self.height && x < self.width {
            let idx = tof_bin * self.height * self.width + y * self.width + x;
            Some(self.data[idx])
        } else {
            None
        }
    }

    /// Increment the count at a specific position.
    #[inline]
    pub fn increment(&mut self, tof_bin: usize, y: usize, x: usize) {
        if tof_bin < self.n_tof_bins && y < self.height && x < self.width {
            let idx = tof_bin * self.height * self.width + y * self.width + x;
            self.data[idx] += 1;
        }
    }

    /// Sum projection over all TOF bins.
    ///
    /// Returns a 2D array (flattened) of shape `[height, width]` containing
    /// the sum of counts across all TOF bins for each pixel.
    #[must_use]
    pub fn project_xy(&self) -> Vec<u64> {
        let xy_size = self.height * self.width;
        let mut result = vec![0u64; xy_size];

        for tof_bin in 0..self.n_tof_bins {
            let start = tof_bin * xy_size;
            let end = start + xy_size;
            for (i, &count) in self.data[start..end].iter().enumerate() {
                result[i] += count;
            }
        }

        result
    }

    /// Get a slice of data at a specific TOF bin.
    ///
    /// Returns a borrowed slice of the XY plane at the given TOF index.
    #[must_use]
    pub fn slice_tof(&self, tof_bin: usize) -> Option<&[u64]> {
        if tof_bin >= self.n_tof_bins {
            return None;
        }

        let xy_size = self.height * self.width;
        let start = tof_bin * xy_size;
        let end = start + xy_size;
        Some(&self.data[start..end])
    }

    /// Compute the TOF spectrum for a spatial ROI.
    ///
    /// Returns a vector of counts per TOF bin, summed over the specified
    /// X and Y ranges.
    #[must_use]
    pub fn spectrum(&self, x_range: Range<usize>, y_range: Range<usize>) -> Vec<u64> {
        let mut result = vec![0u64; self.n_tof_bins];

        let x_start = x_range.start.min(self.width);
        let x_end = x_range.end.min(self.width);
        let y_start = y_range.start.min(self.height);
        let y_end = y_range.end.min(self.height);

        for (tof_bin, bin_count) in result.iter_mut().enumerate() {
            let mut sum = 0u64;
            for y in y_start..y_end {
                for x in x_start..x_end {
                    let idx = tof_bin * self.height * self.width + y * self.width + x;
                    sum += self.data[idx];
                }
            }
            *bin_count = sum;
        }

        result
    }

    /// Compute the full TOF spectrum (sum over all pixels).
    #[must_use]
    pub fn full_spectrum(&self) -> Vec<u64> {
        self.spectrum(0..self.width, 0..self.height)
    }

    /// Get the number of TOF bins.
    #[must_use]
    #[inline]
    pub fn n_tof_bins(&self) -> usize {
        self.n_tof_bins
    }

    /// Get the width (X dimension).
    #[must_use]
    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the height (Y dimension).
    #[must_use]
    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Get the maximum TOF value in 25ns units.
    #[must_use]
    #[inline]
    pub fn tof_max(&self) -> u32 {
        self.tof_max
    }

    /// Get the bin width in 25ns units.
    #[must_use]
    #[inline]
    pub fn bin_width(&self) -> f64 {
        self.bin_width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_hyperstack() {
        let hs = Hyperstack3D::new(10, 8, 8, 1000);
        assert_eq!(hs.n_tof_bins(), 10);
        assert_eq!(hs.width(), 8);
        assert_eq!(hs.height(), 8);
        assert_eq!(hs.data.len(), 10 * 8 * 8);
    }

    #[test]
    fn test_increment_and_get() {
        let mut hs = Hyperstack3D::new(10, 8, 8, 1000);
        hs.increment(5, 3, 2);
        hs.increment(5, 3, 2);
        assert_eq!(hs.get(5, 3, 2), Some(2));
        assert_eq!(hs.get(0, 0, 0), Some(0));
    }

    #[test]
    fn test_project_xy() {
        let mut hs = Hyperstack3D::new(3, 4, 4, 300);
        // Add counts at same pixel in different TOF bins
        hs.increment(0, 1, 1);
        hs.increment(1, 1, 1);
        hs.increment(2, 1, 1);

        let proj = hs.project_xy();
        // Pixel (1,1) should have count 3 (index = y*width + x = 1*4 + 1 = 5)
        assert_eq!(proj[5], 3);
        // Other pixels should be 0
        assert_eq!(proj[0], 0);
    }

    #[test]
    fn test_slice_tof() {
        let mut hs = Hyperstack3D::new(3, 4, 4, 300);
        hs.increment(1, 2, 3);

        let slice = hs.slice_tof(1).unwrap();
        assert_eq!(slice[2 * 4 + 3], 1);

        assert!(hs.slice_tof(10).is_none());
    }

    #[test]
    fn test_spectrum() {
        let mut hs = Hyperstack3D::new(5, 4, 4, 500);
        // Add counts at different TOF bins
        hs.increment(0, 1, 1);
        hs.increment(2, 1, 1);
        hs.increment(2, 1, 1);
        hs.increment(4, 2, 2);

        let spec = hs.spectrum(0..4, 0..4);
        assert_eq!(spec, vec![1, 0, 2, 0, 1]);
    }
}
