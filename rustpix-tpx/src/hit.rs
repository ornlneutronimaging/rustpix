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
    fn test_tof_calculation() {
        let timestamp = 1000u32;
        let tdc_timestamp = 500u32;
        let correction = 666_667_u32; // ~1/60Hz in 25ns units

        let tof = calculate_tof(timestamp, tdc_timestamp, correction);
        assert_eq!(tof, 500);
    }
}
