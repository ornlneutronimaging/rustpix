//! Texture generation for histogram visualization.

use egui::ColorImage;

use crate::viewer::Colormap;

/// Convert u64 to f32 with allowed precision loss.
#[allow(clippy::cast_precision_loss)]
fn u64_to_f32(value: u64) -> f32 {
    value as f32
}

/// Generate a 512x512 color image from hit counts using the specified colormap.
///
/// The image uses sqrt scaling for better dynamic range visualization.
///
/// # Arguments
/// * `counts` - 512×512 pixel count grid (row-major order)
/// * `colormap` - Colormap to apply
///
/// # Returns
/// RGBA color image suitable for display
#[must_use]
pub fn generate_histogram_image(counts: &[u64], colormap: Colormap, log_scale: bool) -> ColorImage {
    // Find max for scaling
    let max_count_u64 = counts.iter().max().copied().unwrap_or(1);
    let max_count = u64_to_f32(max_count_u64.max(1));
    let max_log = if log_scale {
        max_count.log10().max(1.0)
    } else {
        1.0
    };
    let mut pixels = Vec::with_capacity(512 * 512 * 4);

    for &count in counts {
        if count == 0 {
            pixels.extend_from_slice(&[0, 0, 0, 255]);
        } else {
            let val = if log_scale {
                let log_val = u64_to_f32(count.max(1)).log10() / max_log;
                log_val.clamp(0.0, 1.0)
            } else {
                (u64_to_f32(count) / max_count).sqrt() // Sqrt scale
            };
            let rgba = colormap.apply(val);
            pixels.extend_from_slice(&rgba);
        }
    }

    ColorImage::from_rgba_unmultiplied([512, 512], &pixels)
}
