//! Texture generation for histogram visualization.

use egui::ColorImage;

use crate::viewer::Colormap;

/// Convert u64 to f32 with allowed precision loss.
#[allow(clippy::cast_precision_loss)]
fn u64_to_f32(value: u64) -> f32 {
    value as f32
}

/// Generate a color image from hit counts using the specified colormap.
///
/// The image uses sqrt scaling for better dynamic range visualization.
///
/// # Arguments
/// * `counts` - pixel count grid (row-major order)
/// * `width` - image width in pixels
/// * `height` - image height in pixels
/// * `colormap` - Colormap to apply
///
/// # Returns
/// RGBA color image suitable for display
#[must_use]
pub fn generate_histogram_image(
    counts: &[u64],
    width: usize,
    height: usize,
    colormap: Colormap,
    log_scale: bool,
) -> ColorImage {
    // Find max for scaling
    let max_count_u64 = counts.iter().max().copied().unwrap_or(1);
    let max_count = u64_to_f32(max_count_u64.max(1));
    let max_log = if log_scale {
        max_count.log10().max(1.0)
    } else {
        1.0
    };
    let width = width.max(1);
    let height = height.max(1);
    let pixel_count = width.saturating_mul(height);
    let mut pixels = vec![0u8; pixel_count * 4];

    for (idx, &count) in counts.iter().take(pixel_count).enumerate() {
        if count == 0 {
            let offset = idx * 4;
            pixels[offset..offset + 4].copy_from_slice(&[0, 0, 0, 255]);
        } else {
            let val = if log_scale {
                let log_val = u64_to_f32(count.max(1)).log10() / max_log;
                log_val.clamp(0.0, 1.0)
            } else {
                (u64_to_f32(count) / max_count).sqrt() // Sqrt scale
            };
            let rgba = colormap.apply(val);
            let offset = idx * 4;
            pixels[offset..offset + 4].copy_from_slice(&rgba);
        }
    }

    ColorImage::from_rgba_unmultiplied([width, height], &pixels)
}
