//! TPX3 timing helpers.
//!

/// Timestamp rollover correction.
///
/// TPX3 uses 30-bit timestamps that can roll over. This function
/// corrects the hit timestamp relative to the TDC timestamp.
///
/// Formula: if `hit_ts` + 0x400000 < `tdc_ts`, extend by 0x40000000
#[inline]
#[must_use]
pub fn correct_timestamp_rollover(hit_timestamp: u32, tdc_timestamp: u32) -> u32 {
    const EXTENSION_THRESHOLD: u32 = 0x0040_0000;
    const EXTENSION_VALUE: u32 = 0x4000_0000;

    if hit_timestamp.wrapping_add(EXTENSION_THRESHOLD) < tdc_timestamp {
        hit_timestamp.wrapping_add(EXTENSION_VALUE)
    } else {
        hit_timestamp
    }
}

/// Mask of the 30-bit hit and TDC timestamp counters.
const TIMESTAMP_MASK: u32 = 0x3FFF_FFFF;

/// Ticks a hit may follow its TDC: half the 30-bit counter (13.4 s).
///
/// Holds two TDC periods for every frequency `DetectorConfig::validate` accepts.
pub(crate) const AFTER_TDC_WINDOW: u32 = 1 << 29;

/// Ticks from `tdc` forward to `hit` on the 30-bit counter, or `None` when the hit is earlier.
///
/// A hit less than [`AFTER_TDC_WINDOW`] ticks after the TDC counts as after it; the
/// other half of the counter range counts as before it. The comparison is modulo the
/// counter, so it holds across a rollover.
#[inline]
#[must_use]
pub(crate) fn ticks_after(hit: u32, tdc: u32) -> Option<u32> {
    let forward = hit.wrapping_sub(tdc) & TIMESTAMP_MASK;
    (forward < AFTER_TDC_WINDOW).then_some(forward)
}

/// Subtracts one TDC period from a TOF longer than it (a missed TDC packet).
#[inline]
#[must_use]
pub(crate) fn apply_missing_tdc_correction(tof: u32, period_25ns: u32) -> u32 {
    if tof > period_25ns {
        tof - period_25ns
    } else {
        tof
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
/// * `tdc_correction_25ns` - The TDC period in 25ns units (1 / `TDC_frequency`).
///   For SNS (60Hz), this is approximately 666,667 units (16.67ms / 25ns).
#[inline]
#[must_use]
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
    fn test_timestamp_rollover() {
        // Normal case - no correction needed
        assert_eq!(correct_timestamp_rollover(100, 50), 100);

        // Rollover case - extension needed
        let hit_ts = 0x100;
        let tdc_ts = 0x0050_0000;
        let corrected = correct_timestamp_rollover(hit_ts, tdc_ts);
        assert_eq!(corrected, hit_ts + 0x4000_0000);
    }

    #[test]
    fn ticks_after_holds_across_rollover() {
        assert_eq!(ticks_after(1100, 1000), Some(100));
        assert_eq!(ticks_after(999, 1000), None);
        // Hit 0xFF00 ticks after a TDC just before the wrap.
        assert_eq!(ticks_after(0x3FFF_FF00, 0x3FFF_0000), Some(0xFF00));
        // Hit just before the wrap, TDC just after it: the hit is earlier.
        assert_eq!(ticks_after(0x3FFF_FF00, 0x10), None);
        // Hit just after the wrap, TDC just before it.
        assert_eq!(ticks_after(0x20, 0x3FFF_FFF0), Some(0x30));
        // The window is half the counter range.
        assert_eq!(
            ticks_after(1000 + AFTER_TDC_WINDOW - 1, 1000),
            Some(AFTER_TDC_WINDOW - 1)
        );
        assert_eq!(ticks_after(1000 + AFTER_TDC_WINDOW, 1000), None);
    }

    #[test]
    fn test_tof_calculation() {
        let timestamp = 1000u32;
        let tdc_timestamp = 500u32;
        let correction = 666_667_u32; // ~1/60Hz in 25ns units

        let tof = calculate_tof(timestamp, tdc_timestamp, correction);
        assert_eq!(tof, 500);
    }
}
