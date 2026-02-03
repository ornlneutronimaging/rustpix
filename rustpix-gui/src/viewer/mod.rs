//! Visualization modules for histogram display.

mod colormap;
mod roi;
mod texture;

pub use colormap::Colormap;
pub use roi::{Roi, RoiCommitError, RoiHandle, RoiSelectionMode, RoiShape, RoiState};
pub use texture::generate_histogram_image;
