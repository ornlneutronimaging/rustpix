//! Parameter checks and conversions shared by the clustering algorithms.

use rustpix_core::clustering::ClusteringError;

/// Rejects NaN, infinite and negative parameter values.
pub(crate) fn require_finite_non_negative(name: &str, value: f64) -> Result<(), ClusteringError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(ClusteringError::InvalidConfig(format!(
            "{name} must be finite and non-negative, got {value}"
        )))
    }
}

/// Saturating conversion of a non-negative whole number to `u32`.
pub(crate) fn float_to_u32(value: f64) -> u32 {
    if value <= 0.0 {
        return 0;
    }
    if value >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    format!("{value:.0}").parse::<u32>().unwrap_or(u32::MAX)
}

/// Saturating conversion of a non-negative whole number to `usize`.
pub(crate) fn float_to_usize(value: f64) -> usize {
    if value <= 0.0 {
        return 0;
    }
    format!("{value:.0}").parse::<usize>().unwrap_or(usize::MAX)
}

/// Temporal window in 25 ns ticks, rounded up.
pub(crate) fn window_ticks(window_ns: f64) -> u32 {
    float_to_u32((window_ns / 25.0).ceil())
}
