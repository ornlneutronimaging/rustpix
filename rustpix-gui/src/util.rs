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

/// Format a number with comma separators for readability.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(format_number(12345678), "12,345,678");
/// assert_eq!(format_number(1234), "1,234");
/// assert_eq!(format_number(42), "42");
/// ```
#[must_use]
pub fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format a large number with SI suffix (K, M, G).
///
/// # Examples
///
/// ```ignore
/// assert_eq!(format_number_si(1_500_000), "1.50M");
/// assert_eq!(format_number_si(45_000), "45.0K");
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_number_si(n: usize) -> String {
    let n_f64 = n as f64;
    if n >= 1_000_000_000 {
        format!("{:.2}G", n_f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n_f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n_f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Format bytes as a human-readable string (base 1024).
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const TB: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

    let bytes_f64 = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes_f64 < MB {
        format!("{:.1} KB", bytes_f64 / KB)
    } else if bytes_f64 < GB {
        format!("{:.1} MB", bytes_f64 / MB)
    } else if bytes_f64 < TB {
        format!("{:.2} GB", bytes_f64 / GB)
    } else {
        format!("{:.2} TB", bytes_f64 / TB)
    }
}

/// Neutron mass in kilograms.
const NEUTRON_MASS_KG: f64 = 1.674_927_498e-27;
/// Elementary charge in joules per eV.
const EV_J: f64 = 1.602_176_634e-19;

/// Convert TOF (µs) to neutron energy (eV).
///
/// Returns `None` if the input is invalid or results in non-physical values.
#[must_use]
pub fn tof_us_to_energy_ev(tof_us: f64, flight_path_m: f64, tof_offset_ns: f64) -> Option<f64> {
    if !tof_us.is_finite() || !flight_path_m.is_finite() || flight_path_m <= 0.0 {
        return None;
    }
    let offset_us = tof_offset_ns / 1000.0;
    let t_us = tof_us - offset_us;
    if t_us <= 0.0 {
        return None;
    }
    let time_seconds = t_us * 1e-6;
    let v = flight_path_m / time_seconds;
    let e_j = 0.5 * NEUTRON_MASS_KG * v * v;
    Some(e_j / EV_J)
}

/// Convert neutron energy (eV) to TOF (µs).
///
/// Returns `None` if the input is invalid or results in non-physical values.
#[must_use]
pub fn energy_ev_to_tof_us(energy_ev: f64, flight_path_m: f64, tof_offset_ns: f64) -> Option<f64> {
    if !energy_ev.is_finite()
        || energy_ev <= 0.0
        || !flight_path_m.is_finite()
        || flight_path_m <= 0.0
    {
        return None;
    }
    let e_j = energy_ev * EV_J;
    let time_seconds = flight_path_m * (NEUTRON_MASS_KG / (2.0 * e_j)).sqrt();
    let offset_us = tof_offset_ns / 1000.0;
    Some(time_seconds * 1e6 + offset_us)
}

/// Convert TOF (ms) to neutron energy (eV).
#[must_use]
pub fn tof_ms_to_energy_ev(tof_ms: f64, flight_path_m: f64, tof_offset_ns: f64) -> Option<f64> {
    if !tof_ms.is_finite() {
        return None;
    }
    tof_us_to_energy_ev(tof_ms * 1000.0, flight_path_m, tof_offset_ns)
}

/// Convert neutron energy (eV) to TOF (ms).
#[must_use]
pub fn energy_ev_to_tof_ms(energy_ev: f64, flight_path_m: f64, tof_offset_ns: f64) -> Option<f64> {
    energy_ev_to_tof_us(energy_ev, flight_path_m, tof_offset_ns).map(|us| us / 1000.0)
}
