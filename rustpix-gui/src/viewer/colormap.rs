//! Colormap definitions and application logic.

use crate::util::f32_to_u8;

/// Available colormaps for histogram visualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colormap {
    /// Green (Matrix style) - black to bright green.
    Green,
    /// Hot (Thermal) - black to red to yellow to white.
    Hot,
    /// Grayscale - black to white.
    Grayscale,
    /// Viridis (approximate) - blue to teal to green to yellow.
    Viridis,
}

impl std::fmt::Display for Colormap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Colormap::Green => write!(f, "Green (Matrix)"),
            Colormap::Hot => write!(f, "Hot (Thermal)"),
            Colormap::Grayscale => write!(f, "Grayscale"),
            Colormap::Viridis => write!(f, "Viridis"),
        }
    }
}

impl Colormap {
    /// Apply the colormap to a normalized value [0, 1] and return RGBA bytes.
    ///
    /// # Arguments
    /// * `val` - Normalized value between 0.0 and 1.0
    ///
    /// # Returns
    /// RGBA color as `[r, g, b, a]` bytes
    #[must_use]
    pub fn apply(self, val: f32) -> [u8; 4] {
        match self {
            Colormap::Green => {
                let v = f32_to_u8(val * 255.0);
                [0, v, 0, 255]
            }
            Colormap::Grayscale => {
                let v = f32_to_u8(val * 255.0);
                [v, v, v, 255]
            }
            Colormap::Hot => {
                // Simple Red-Yellow-White heatmap
                if val < 0.5 {
                    // Red to Yellow
                    let r = 255;
                    let g = f32_to_u8(val * 2.0 * 255.0);
                    [r, g, 0, 255]
                } else {
                    // Yellow to White
                    let r = 255;
                    let g = 255;
                    let b = f32_to_u8((val - 0.5) * 2.0 * 255.0);
                    [r, g, b, 255]
                }
            }
            Colormap::Viridis => {
                // Approximate Viridis (Blue -> Teal -> Green -> Yellow)
                let r = f32_to_u8(255.0 * val.powf(2.0));
                let g = f32_to_u8(255.0 * val);
                let b = f32_to_u8(255.0 * (1.0 - val));
                [r, g, b, 255]
            }
        }
    }
}
