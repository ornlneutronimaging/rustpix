//! Texture generation for histogram visualization.

use egui::ColorImage;

use crate::state::ViewTransform;
use crate::viewer::Colormap;

/// Convert u64 to f32 with allowed precision loss.
#[allow(clippy::cast_precision_loss)]
fn u64_to_f32(value: u64) -> f32 {
    value as f32
}

/// Generate a color image from hit counts with a display transform applied.
#[must_use]
pub fn generate_histogram_image_transformed(
    counts: &[u64],
    width: usize,
    height: usize,
    transform: ViewTransform,
    colormap: Colormap,
    log_scale: bool,
) -> ColorImage {
    let max_count_u64 = counts.iter().max().copied().unwrap_or(1);
    let max_count = u64_to_f32(max_count_u64.max(1));
    let max_log = if log_scale {
        (max_count + 1.0).log10()
    } else {
        1.0
    };

    let (disp_w, disp_h) = transform.display_size(width.max(1), height.max(1));
    let pixel_count = disp_w.saturating_mul(disp_h);
    let mut pixels = vec![0u8; pixel_count * 4];

    for y in 0..disp_h {
        for x in 0..disp_w {
            let idx = y * disp_w + x;
            let count = transform
                .apply_inverse(x, y, width, height)
                .and_then(|(sx, sy)| counts.get(sy * width + sx).copied())
                .unwrap_or(0);
            if count == 0 {
                let offset = idx * 4;
                pixels[offset..offset + 4].copy_from_slice(&[0, 0, 0, 255]);
            } else {
                let val = if log_scale {
                    let log_val = (u64_to_f32(count) + 1.0).log10() / max_log;
                    log_val.clamp(0.0, 1.0)
                } else {
                    (u64_to_f32(count) / max_count).sqrt()
                };
                let rgba = colormap.apply(val);
                let offset = idx * 4;
                pixels[offset..offset + 4].copy_from_slice(&rgba);
            }
        }
    }

    ColorImage::from_rgba_unmultiplied([disp_w, disp_h], &pixels)
}
