//! Histogram data structures for 3D hyperstack visualization.
//!
//! This module provides the `Hyperstack3D` structure which stores
//! binned event data in a 3D array indexed by `[tof, y, x]`.

use std::ops::Range;

use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;

/// Smallest TOF bin count the UI will offer.
pub const MIN_TOF_BINS: usize = 10;

/// Largest TOF bin count the UI will offer.
///
/// This is a rail against fat-fingered input. It is deliberately not the
/// information limit and not a memory limit, because neither of those is a
/// constant:
///
/// * Information. Bins finer than the detector's 25 ns TOF quantum hold no
///   extra signal, so the useful ceiling is `tof_max` in 25 ns units — but
///   that scales with the TDC frequency: ~666,667 at 60 Hz, and 40,000,000
///   at the 1 Hz floor the GUI allows. This constant clears the 60 Hz case
///   outright, so a VENUS measurement is never stopped short of the
///   detector's own resolution.
/// * Memory. The hyperstack is dense, so cost is linear in bin count — see
///   [`hyperstack_bytes`]. A 514x514 VENUS detector costs ~2.1 MB per bin,
///   and the hits and neutrons stacks are resident at the same time.
///
/// Memory is what actually binds in practice, and by how much depends on the
/// detector and the host, so the UI reports the estimate next to the control
/// instead of baking in a number that would be wrong on most machines.
pub const MAX_TOF_BINS: usize = 1_000_000;

/// Bytes the backing store of a hyperstack of these dimensions will need.
///
/// Saturates rather than overflowing: this feeds a UI estimate, and a
/// clamped "very large" reads the same as an exact one at that scale.
#[must_use]
pub fn hyperstack_bytes(n_tof_bins: usize, width: usize, height: usize) -> u64 {
    let elem = u64::try_from(std::mem::size_of::<u64>()).unwrap_or(u64::MAX);
    u64::try_from(n_tof_bins)
        .unwrap_or(u64::MAX)
        .saturating_mul(u64::try_from(width).unwrap_or(u64::MAX))
        .saturating_mul(u64::try_from(height).unwrap_or(u64::MAX))
        .saturating_mul(elem)
}

/// A 3D histogram storing counts indexed by (TOF bin, y, x).
///
/// Data is stored in row-major order: `data[tof * height * width + y * width + x]`
///
/// # Memory Layout
///
/// Storage is dense, so it grows linearly with the bin count: a
/// 200-bin × 512 × 512 hyperstack is 419.4 MB (400 MiB), and 10,000 bins on
/// a 514 × 514 VENUS detector is 21.1 GB (19.7 GiB). Use
/// [`hyperstack_bytes`] to size one before building it. Note the UI's
/// `format_bytes` renders these in binary units, so the settings window
/// shows the latter as "19.68 GB".
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
        hyperstack.accumulate_hits(batch);

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
    #[cfg(test)]
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
    #[cfg(test)]
    #[inline]
    pub fn increment(&mut self, tof_bin: usize, y: usize, x: usize) {
        if tof_bin < self.n_tof_bins && y < self.height && x < self.width {
            let idx = tof_bin * self.height * self.width + y * self.width + x;
            self.data[idx] += 1;
        }
    }

    /// Accumulate a batch of hits into the hyperstack.
    pub fn accumulate_hits(&mut self, batch: &HitBatch) {
        if self.n_tof_bins == 0 || self.width == 0 || self.height == 0 {
            return;
        }

        let width = self.width;
        let height = self.height;
        let n_bins = self.n_tof_bins;
        let bin_width = self.bin_width;

        for i in 0..batch.len() {
            let x = usize::from(batch.x[i]);
            let y = usize::from(batch.y[i]);
            let tof = batch.tof[i];

            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let tof_bin = if bin_width > 0.0 {
                let bin = (f64::from(tof) / bin_width) as usize;
                bin.min(n_bins.saturating_sub(1))
            } else {
                0
            };

            if x < width && y < height && tof_bin < n_bins {
                let idx = tof_bin * height * width + y * width + x;
                self.data[idx] += 1;
            }
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

    /// Access the flattened counts array (`[tof, y, x]` order).
    #[must_use]
    pub fn data(&self) -> &[u64] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VENUS TOF span for a 60 Hz source, in 25 ns units.
    const VENUS_TOF_MAX: u32 = 666_667;

    #[test]
    fn tof_bin_range_admits_ten_thousand_bins() {
        // The instrument scientists asked for 10,000 bins; the old cap was 2,000.
        // This is the same range expression the DragValues are built with, so it
        // fails if either bound stops admitting the requested count.
        assert!((MIN_TOF_BINS..=MAX_TOF_BINS).contains(&10_000));
        // Real headroom above the request, not a cap moved just far enough.
        assert!((MIN_TOF_BINS..=MAX_TOF_BINS).contains(&100_000));
    }

    #[test]
    fn tof_bin_ceiling_covers_full_25ns_resolution_at_60hz() {
        // A 60 Hz source spans VENUS_TOF_MAX ticks of 25 ns. Keeping the ceiling
        // at or above that means the UI never stops a user short of the
        // detector's own TOF resolution. (At lower TDC frequencies tof_max is
        // larger still — the ceiling is a typo rail, not an information limit.)
        assert!(MAX_TOF_BINS >= VENUS_TOF_MAX as usize);
    }

    #[test]
    fn tof_binning_is_correct_at_ten_thousand_bins() {
        // 1x1 detector keeps this cheap (80 KB) while still exercising the
        // narrow-bin arithmetic that only shows up at high bin counts.
        let bins = 10_000;
        let mut hs = Hyperstack3D::new(bins, 1, 1, VENUS_TOF_MAX);
        // 666_667 / 10_000 = 66.6667 units per bin — still well above the
        // 1-unit (25 ns) hardware quantum, so no bin is unreachable.
        assert!(hs.bin_width() > 1.0);

        let mut batch = HitBatch::default();
        // Bins 0 and 9_999 alone would not catch a wrong bin width: bin 0
        // absorbs everything that lands too low, and the `.min(n_bins - 1)`
        // clamp absorbs everything that lands too high. The middle probe is the
        // one that pins the scale. Each value sits mid-bin, ~33 units from
        // either edge, so the test keys on the bin width and not on rounding.
        batch.push((0, 0, 33, 10, 0, 0)); // bin 0     [0, 66.67)
        batch.push((0, 0, 333_300, 10, 0, 0)); // bin 4_999 [333_266.83, 333_333.50)
        batch.push((0, 0, 666_633, 10, 0, 0)); // bin 9_999 [666_600.33, 666_667.00)
        hs.accumulate_hits(&batch);

        assert_eq!(hs.get(0, 0, 0), Some(1));
        assert_eq!(hs.get(4_999, 0, 0), Some(1));
        assert_eq!(hs.get(9_999, 0, 0), Some(1));

        // Neighbours of the middle probe must be empty, or a bin width that is
        // merely close would still pass.
        assert_eq!(hs.get(4_998, 0, 0), Some(0));
        assert_eq!(hs.get(5_000, 0, 0), Some(0));

        let spectrum = hs.full_spectrum();
        assert_eq!(spectrum.len(), bins);
        assert_eq!(spectrum.iter().sum::<u64>(), 3);
    }

    #[test]
    fn hyperstack_bytes_matches_documented_sizes() {
        // The figures quoted in the Hyperstack3D docs.
        assert_eq!(hyperstack_bytes(200, 512, 512), 419_430_400);
        // 514x514 VENUS at the requested 10,000 bins.
        assert_eq!(hyperstack_bytes(10_000, 514, 514), 21_135_680_000);
    }

    #[test]
    fn hyperstack_bytes_saturates_instead_of_overflowing() {
        assert_eq!(
            hyperstack_bytes(usize::MAX, usize::MAX, usize::MAX),
            u64::MAX
        );
    }

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
