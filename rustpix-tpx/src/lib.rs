//! rustpix-tpx: TPX3 packet parser, hit types, and file processor.
//!
//! This crate provides TPX3-specific data structures and parsing logic
//! for Timepix3 pixel detector data.
//!
//! See IMPLEMENTATION_PLAN.md Part 3 for detailed specification.
//!
//! # Key Components
//!
//! - [`Tpx3Packet`] - Low-level packet parser with bit field extraction
//! - [`Tpx3Hit`] - Hit data structure with TOF, coordinates, and cluster assignment
//! - `Tpx3Processor` - Section-aware file processor (TODO)
//!
//! # Processing Pipeline
//!
//! 1. **Phase 1 (Sequential)**: Discover sections, propagate TDC state
//! 2. **Phase 2 (Parallel)**: Process sections into hits
//!
//! See IMPLEMENTATION_PLAN.md Part 3.3-3.4 for algorithm details.

mod hit;
mod packet;
pub mod section;

pub use hit::{calculate_tof, correct_timestamp_rollover, Tpx3Hit};
pub use packet::Tpx3Packet;

// Re-export core types for convenience
pub use rustpix_core::hit::Hit;

// TODO: Implement these components (see IMPLEMENTATION_PLAN.md Part 3):
//
// - Section discovery and TDC propagation (Part 3.3)
// - Tpx3Processor with parallel processing (Part 3.4)
// - DetectorConfig with chip transforms
// - Memory-mapped file I/O integration

/// Detector configuration for TPX3 processing.
///
/// TODO: Full implementation in IMPLEMENTATION_PLAN.md
#[derive(Clone, Debug)]
pub struct DetectorConfig {
    /// TDC frequency in Hz (default: 60.0 for SNS).
    pub tdc_frequency_hz: f64,
    /// Enable missing TDC correction.
    pub enable_missing_tdc_correction: bool,
    /// Chip size in pixels (default: 256).
    pub chip_size: u16,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self::venus_defaults()
    }
}

impl DetectorConfig {
    /// Create VENUS/SNS default configuration.
    pub fn venus_defaults() -> Self {
        Self {
            tdc_frequency_hz: 60.0,
            enable_missing_tdc_correction: true,
            chip_size: 256,
        }
    }

    /// TDC period in seconds.
    pub fn tdc_period_seconds(&self) -> f64 {
        1.0 / self.tdc_frequency_hz
    }

    /// TDC correction value in 25ns units.
    pub fn tdc_correction_25ns(&self) -> u32 {
        (self.tdc_period_seconds() / 25e-9).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_venus_defaults() {
        let config = DetectorConfig::venus_defaults();
        assert_eq!(config.tdc_frequency_hz, 60.0);
        assert!(config.enable_missing_tdc_correction);
    }

    #[test]
    fn test_tdc_correction() {
        let config = DetectorConfig::venus_defaults();
        // 1/60 Hz = 16.67ms, in 25ns units = 666,667
        let correction = config.tdc_correction_25ns();
        assert!(correction > 600_000 && correction < 700_000);
    }
}
