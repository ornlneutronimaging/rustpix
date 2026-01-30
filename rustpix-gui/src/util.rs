//! Numeric conversion utilities for rustpix-gui.
//!
//! These functions handle conversions between numeric types with explicit
//! handling of precision loss and bounds checking.

/// Convert usize to f32 with allowed precision loss.
#[allow(clippy::cast_precision_loss)]
pub fn usize_to_f32(value: usize) -> f32 {
    value as f32
}

/// Convert usize to f64 with allowed precision loss.
#[allow(clippy::cast_precision_loss)]
pub fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

/// Convert u64 to f64 with allowed precision loss.
#[allow(clippy::cast_precision_loss)]
pub fn u64_to_f64(value: u64) -> f64 {
    value as f64
}

/// Convert f32 to u8 with clamping to [0, 255].
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn f32_to_u8(value: f32) -> u8 {
    let clamped = value.clamp(0.0, 255.0);
    clamped.round() as u8
}

/// Convert f64 to usize with bounds checking.
///
/// Returns `None` if the value is not finite, negative, or >= `max_exclusive`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn f64_to_usize_bounded(value: f64, max_exclusive: usize) -> Option<usize> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let max_f64 = usize_to_f64(max_exclusive);
    if value >= max_f64 {
        return None;
    }
    Some(value as usize)
}
