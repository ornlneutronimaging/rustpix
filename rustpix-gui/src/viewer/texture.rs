//! Texture generation for histogram visualization.

use egui::ColorImage;

use crate::util::u32_to_f32;
use crate::viewer::Colormap;

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
pub fn generate_histogram_image(counts: &[u32], colormap: Colormap) -> ColorImage {
    // Find max for scaling
    let max_count = u32_to_f32(counts.iter().max().copied().unwrap_or(1));
    let mut pixels = Vec::with_capacity(512 * 512 * 4);

    for &count in counts {
        if count == 0 {
            pixels.extend_from_slice(&[0, 0, 0, 255]);
        } else {
            let val = (u32_to_f32(count) / max_count).sqrt(); // Sqrt scale
            let rgba = colormap.apply(val);
            pixels.extend_from_slice(&rgba);
        }
    }

    ColorImage::from_rgba_unmultiplied([512, 512], &pixels)
}
