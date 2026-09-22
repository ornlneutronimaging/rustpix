//! Persisted layout preferences (currently the spectrum section height).
//!
//! Stored as `key=value` lines in `~/.config/venus_rust_tools/rustpix_layout`.
//! Persistence is best effort, like the recent-files list: an unwritable home
//! directory only loses the preference, it never errors.

use std::path::PathBuf;

/// Default spectrum section height in points (toolbar + plot + legend).
pub const DEFAULT_SPECTRUM_HEIGHT: f32 = 240.0;
/// Smallest useful spectrum section height.
pub const MIN_SPECTRUM_HEIGHT: f32 = 120.0;
/// Cap so a stale preference can never push the image off-screen entirely.
pub const MAX_SPECTRUM_HEIGHT: f32 = 1200.0;

/// The preferences file, under `$XDG_CONFIG_HOME` (or `~/.config`).
fn prefs_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("venus_rust_tools").join("rustpix_layout"))
}

/// Load the saved spectrum height, clamped to the valid range; the default
/// when the file is missing or unreadable.
pub fn load_spectrum_height() -> f32 {
    let Some(text) = prefs_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return DEFAULT_SPECTRUM_HEIGHT;
    };
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("spectrum_height="))
        .find_map(|v| v.trim().parse::<f32>().ok())
        .filter(|h| h.is_finite())
        .map_or(DEFAULT_SPECTRUM_HEIGHT, |h| {
            h.clamp(MIN_SPECTRUM_HEIGHT, MAX_SPECTRUM_HEIGHT)
        })
}

pub fn save_spectrum_height(height: f32) {
    let Some(path) = prefs_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("spectrum_height={height:.0}\n"));
}
