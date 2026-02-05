//! UI state for panel visibility and view options.

use std::fmt;

use eframe::egui::Rect;
use egui_plot::{PlotBounds, PlotPoint};
/// Data source for the main viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// View raw hit events.
    #[default]
    Hits,
    /// View clustered neutron events.
    Neutrons,
}

impl fmt::Display for ViewMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hits => write!(f, "Hits"),
            Self::Neutrons => write!(f, "Neutrons"),
        }
    }
}

/// X-axis mode for the spectrum plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpectrumXAxis {
    /// Time-of-flight in milliseconds.
    #[default]
    ToFMs,
    /// Neutron energy in eV.
    EnergyEv,
}

impl fmt::Display for SpectrumXAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ToFMs => write!(f, "TOF (ms)"),
            Self::EnergyEv => write!(f, "Energy (eV)"),
        }
    }
}

/// Zoom tool mode for plot navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoomMode {
    #[default]
    None,
    In,
    Out,
    Box,
}

/// UI panel visibility and toggle state.
#[derive(Default)]
pub struct UiState {
    /// Histogram-specific toggles.
    pub histogram: UiHistogramToggles,
    /// Histogram view flags.
    pub histogram_view: UiHistogramView,
    /// Spectrum-specific toggles.
    pub spectrum: UiSpectrumToggles,
    /// Panel visibility toggles.
    pub panels: UiPanelToggles,
    /// Panel popover visibility toggles.
    pub panel_popups: UiPanelPopups,
    /// Pixel health toggles.
    pub pixel_health: UiPixelHealthToggles,
    /// Cache settings.
    pub cache: UiCacheToggles,
    /// Export dialog state.
    pub export: UiExportState,
    /// Current TOF bin index for slicer view.
    pub current_tof_bin: usize,
    /// Current data source (Hits or Neutrons).
    pub view_mode: ViewMode,
    /// Spectrum X-axis mode.
    pub spectrum_x_axis: SpectrumXAxis,
    /// Transient ROI warning message (text, expires at time).
    pub roi_warning: Option<(String, f64)>,
    /// Transient ROI status message (text, expires at time).
    pub roi_status: Option<(String, f64)>,
    /// Cached plot bounds for ROI hit-testing before plot interaction.
    pub roi_last_plot_bounds: Option<PlotBounds>,
    /// Cached plot rect for ROI hit-testing before plot interaction.
    pub roi_last_plot_rect: Option<Rect>,
    /// Cached plot bounds for spectrum interactions.
    pub spectrum_last_plot_bounds: Option<PlotBounds>,
    /// Cached plot rect for spectrum interactions.
    pub spectrum_last_plot_rect: Option<Rect>,
    /// Active zoom tool for the histogram view.
    pub hist_zoom_mode: ZoomMode,
    /// Active zoom tool for the spectrum view.
    pub spectrum_zoom_mode: ZoomMode,
    /// Histogram zoom box drag start in plot coordinates.
    pub hist_zoom_start: Option<PlotPoint>,
    /// Spectrum zoom box drag start in plot coordinates.
    pub spectrum_zoom_start: Option<PlotPoint>,
    /// Spectrum X range override (min, max) in axis units.
    pub spectrum_x_range: Option<(f64, f64)>,
    /// Spectrum Y range override (min, max) in axis units.
    pub spectrum_y_range: Option<(f64, f64)>,
    /// Editable input for spectrum X min.
    pub spectrum_x_min_input: String,
    /// Editable input for spectrum X max.
    pub spectrum_x_max_input: String,
    /// Editable input for spectrum Y min.
    pub spectrum_y_min_input: String,
    /// Editable input for spectrum Y max.
    pub spectrum_y_max_input: String,
    /// ROI currently being renamed.
    pub roi_rename_id: Option<usize>,
    /// Editable name buffer for ROI renaming.
    pub roi_rename_text: String,
}

#[derive(Clone, Copy, Default)]
pub struct UiHistogramToggles {
    /// Whether the TOF histogram window is visible.
    pub show: bool,
    /// Whether slicer mode is enabled (show single TOF slice vs full projection).
    pub slicer_enabled: bool,
    /// Whether to apply log scale to the histogram view.
    pub log_scale: bool,
}

#[derive(Clone, Copy, Default)]
pub struct UiHistogramView {
    /// Whether to show grid lines in the image viewer.
    pub show_grid: bool,
    /// Flag to trigger plot bounds reset (auto-fit to data).
    pub needs_plot_reset: bool,
    /// Current histogram view transform.
    pub transform: ViewTransform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewTransform {
    pub rotation: Rotation,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            rotation: Rotation::R0,
            flip_h: false,
            flip_v: false,
        }
    }
}

impl ViewTransform {
    #[must_use]
    pub fn is_identity(self) -> bool {
        self.rotation == Rotation::R0 && !self.flip_h && !self.flip_v
    }

    pub fn rotate_cw(&mut self) {
        self.rotation = match self.rotation {
            Rotation::R0 => Rotation::R90,
            Rotation::R90 => Rotation::R180,
            Rotation::R180 => Rotation::R270,
            Rotation::R270 => Rotation::R0,
        };
    }

    pub fn rotate_ccw(&mut self) {
        self.rotation = match self.rotation {
            Rotation::R0 => Rotation::R270,
            Rotation::R90 => Rotation::R0,
            Rotation::R180 => Rotation::R90,
            Rotation::R270 => Rotation::R180,
        };
    }

    pub fn flip_horizontal(&mut self) {
        self.flip_h = !self.flip_h;
    }

    pub fn flip_vertical(&mut self) {
        self.flip_v = !self.flip_v;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[must_use]
    pub fn display_size(self, width: usize, height: usize) -> (usize, usize) {
        match self.rotation {
            Rotation::R90 | Rotation::R270 => (height, width),
            _ => (width, height),
        }
    }

    #[must_use]
    pub fn apply_inverse(
        self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> Option<(usize, usize)> {
        if width == 0 || height == 0 {
            return None;
        }
        let (disp_w, disp_h) = self.display_size(width, height);
        if x >= disp_w || y >= disp_h {
            return None;
        }
        let mut x = x;
        let mut y = y;
        if self.flip_h {
            x = disp_w.saturating_sub(1).saturating_sub(x);
        }
        if self.flip_v {
            y = disp_h.saturating_sub(1).saturating_sub(y);
        }
        let (src_x, src_y) = match self.rotation {
            Rotation::R0 => (x, y),
            Rotation::R90 => (y, height.saturating_sub(1).saturating_sub(x)),
            Rotation::R180 => (
                width.saturating_sub(1).saturating_sub(x),
                height.saturating_sub(1).saturating_sub(y),
            ),
            Rotation::R270 => (width.saturating_sub(1).saturating_sub(y), x),
        };
        if src_x >= width || src_y >= height {
            return None;
        }
        Some((src_x, src_y))
    }

    #[must_use]
    pub fn apply_f64(self, x: f64, y: f64, width: f64, height: f64) -> Option<(f64, f64)> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let (mut out_x, mut out_y) = match self.rotation {
            Rotation::R0 => (x, y),
            Rotation::R90 => (height - y, x),
            Rotation::R180 => (width - x, height - y),
            Rotation::R270 => (y, width - x),
        };
        let (disp_w, disp_h) = match self.rotation {
            Rotation::R90 | Rotation::R270 => (height, width),
            _ => (width, height),
        };
        if self.flip_h {
            out_x = disp_w - out_x;
        }
        if self.flip_v {
            out_y = disp_h - out_y;
        }
        Some((out_x, out_y))
    }

    #[must_use]
    pub fn apply_inverse_f64(self, x: f64, y: f64, width: f64, height: f64) -> Option<(f64, f64)> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let (disp_w, disp_h) = match self.rotation {
            Rotation::R90 | Rotation::R270 => (height, width),
            _ => (width, height),
        };
        let mut x = x;
        let mut y = y;
        if self.flip_h {
            x = disp_w - x;
        }
        if self.flip_v {
            y = disp_h - y;
        }
        let (src_x, src_y) = match self.rotation {
            Rotation::R0 => (x, y),
            Rotation::R90 => (y, height - x),
            Rotation::R180 => (width - x, height - y),
            Rotation::R270 => (width - y, x),
        };
        Some((src_x, src_y))
    }

    #[must_use]
    pub fn status_label(self) -> Option<String> {
        if self.is_identity() {
            return None;
        }
        let mut parts = Vec::new();
        match self.rotation {
            Rotation::R0 => {}
            Rotation::R90 => parts.push("Rot 90° CW".to_string()),
            Rotation::R180 => parts.push("Rot 180°".to_string()),
            Rotation::R270 => parts.push("Rot 90° CCW".to_string()),
        }
        match (self.flip_h, self.flip_v) {
            (true, true) => parts.push("Flip H+V".to_string()),
            (true, false) => parts.push("Flip H".to_string()),
            (false, true) => parts.push("Flip V".to_string()),
            (false, false) => {}
        }
        Some(parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::{Rotation, ViewTransform};
    use std::collections::HashSet;

    fn assert_close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {a} ≈ {b}");
    }

    #[test]
    fn view_transform_apply_inverse_is_bijection() {
        let width = 4usize;
        let height = 3usize;
        let rotations = [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270];
        for rotation in rotations {
            for flip_h in [false, true] {
                for flip_v in [false, true] {
                    let transform = ViewTransform {
                        rotation,
                        flip_h,
                        flip_v,
                    };
                    let (disp_w, disp_h) = transform.display_size(width, height);
                    let mut seen = HashSet::with_capacity(width * height);
                    for y in 0..disp_h {
                        for x in 0..disp_w {
                            let (sx, sy) = transform
                                .apply_inverse(x, y, width, height)
                                .expect("in-bounds coords must map");
                            assert!(sx < width && sy < height);
                            let idx = sy * width + sx;
                            assert!(seen.insert(idx), "duplicate mapping for {idx}");
                        }
                    }
                    assert_eq!(seen.len(), width * height);
                    assert!(transform.apply_inverse(disp_w, 0, width, height).is_none());
                    assert!(transform.apply_inverse(0, disp_h, width, height).is_none());
                }
            }
        }
    }

    #[test]
    fn view_transform_f64_round_trip() {
        let width = 5.0;
        let height = 3.0;
        let points = [(0.0, 0.0), (0.5, 0.5), (1.25, 2.75), (4.2, 0.1), (5.0, 3.0)];
        let rotations = [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270];
        for rotation in rotations {
            for flip_h in [false, true] {
                for flip_v in [false, true] {
                    let transform = ViewTransform {
                        rotation,
                        flip_h,
                        flip_v,
                    };
                    for (x, y) in points {
                        let (dx, dy) = transform
                            .apply_f64(x, y, width, height)
                            .expect("valid dims");
                        let (sx, sy) = transform
                            .apply_inverse_f64(dx, dy, width, height)
                            .expect("valid dims");
                        assert_close(sx, x);
                        assert_close(sy, y);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct UiSpectrumToggles {
    /// Whether to use log scale for X axis in spectrum.
    pub log_x: bool,
    /// Whether to use log scale for Y axis in spectrum.
    pub log_y: bool,
    /// Whether the full-FOV spectrum is visible.
    pub full_fov_visible: bool,
}

#[derive(Clone, Copy, Default)]
pub struct UiPanelToggles {
    /// Whether to show advanced clustering parameters.
    pub show_clustering_params: bool,
    /// Whether to show the app settings window.
    pub show_app_settings: bool,
    /// Whether to show the spectrum settings window.
    pub show_spectrum_settings: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Default)]
pub struct UiPanelPopups {
    /// Whether the spectrum data selection panel is open.
    pub show_roi_panel: bool,
    /// Whether the spectrum range panel is open.
    pub show_spectrum_range: bool,
    /// Whether the ROI help panel is open.
    pub show_roi_help: bool,
    /// Whether the clustering help panel is open.
    pub show_clustering_help: bool,
    /// Whether the view help panel is open.
    pub show_view_help: bool,
    /// Whether the pixel health help panel is open.
    pub show_pixel_health_help: bool,
    /// Whether the spectrum help panel is open.
    pub show_spectrum_help: bool,
}

#[derive(Clone, Copy, Default)]
pub struct UiPixelHealthToggles {
    /// Whether to show advanced pixel health settings.
    pub show_pixel_health_settings: bool,
    /// Whether to show hot pixel overlay in the viewer.
    pub show_hot_pixels: bool,
    /// Whether to exclude masked pixels from spectra/statistics.
    pub exclude_masked_pixels: bool,
}

#[derive(Clone, Copy, Default)]
pub struct UiCacheToggles {
    /// Whether to cache raw hits in memory (enables rebuild/export).
    pub cache_hits_in_memory: bool,
}

#[derive(Default)]
pub struct UiExportState {
    /// Whether the export dialog is open.
    pub show_dialog: bool,
    /// Whether an export is in progress.
    pub in_progress: bool,
    /// Export progress value from 0.0 to 1.0.
    pub progress: f32,
    /// Export status message.
    pub status: String,
    /// Selected export format.
    pub format: ExportFormat,
    /// HDF5 export configuration.
    pub options: Hdf5ExportOptions,
    /// TIFF export configuration.
    pub tiff: TiffExportOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Hdf5,
    TiffFolder,
    TiffStack,
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hdf5 => write!(f, "HDF5 (NeXus)"),
            Self::TiffFolder => write!(f, "TIFF Folder"),
            Self::TiffStack => write!(f, "TIFF Stack"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TiffBitDepth {
    #[default]
    Bit16,
    Bit32,
}

impl fmt::Display for TiffBitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bit16 => write!(f, "16-bit"),
            Self::Bit32 => write!(f, "32-bit"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TiffSpectraTiming {
    #[default]
    BinCenter,
    BinStart,
}

impl fmt::Display for TiffSpectraTiming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinCenter => write!(f, "Bin center"),
            Self::BinStart => write!(f, "Bin start"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TiffStackBehavior {
    #[default]
    StandardOnly,
    AutoBigTiff,
    AlwaysBigTiff,
}

impl fmt::Display for TiffStackBehavior {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StandardOnly => write!(f, "Standard TIFF (most compatible)"),
            Self::AutoBigTiff => write!(f, "Auto BigTIFF if needed"),
            Self::AlwaysBigTiff => write!(f, "Always BigTIFF"),
        }
    }
}

#[derive(Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct TiffExportOptions {
    pub bit_depth: TiffBitDepth,
    pub include_spectra: bool,
    pub include_summed_image: bool,
    pub exclude_masked_pixels: bool,
    pub include_tof_offset: bool,
    pub spectra_timing: TiffSpectraTiming,
    pub base_name: String,
    pub stack_behavior: TiffStackBehavior,
}

impl Default for TiffExportOptions {
    fn default() -> Self {
        Self {
            bit_depth: TiffBitDepth::Bit16,
            include_spectra: true,
            include_summed_image: true,
            exclude_masked_pixels: true,
            include_tof_offset: true,
            spectra_timing: TiffSpectraTiming::BinCenter,
            base_name: "Run_XXXXX".to_string(),
            stack_behavior: TiffStackBehavior::StandardOnly,
        }
    }
}

#[derive(Clone)]
pub struct Hdf5ExportOptions {
    pub datasets: Hdf5ExportDatasets,
    pub masks: Hdf5ExportMasks,
    pub advanced: Hdf5ExportAdvancedFlags,
    pub compression_level: u8,
    pub chunk_events: usize,
    pub fields: Hdf5ExportFields,
    pub cluster_fields: Hdf5ExportClusterFields,
    pub hist_chunk_rot: usize,
    pub hist_chunk_y: usize,
    pub hist_chunk_x: usize,
    pub hist_chunk_tof: usize,
}

#[derive(Clone, Copy, Default)]
pub struct Hdf5ExportDatasets {
    pub hits: bool,
    pub neutrons: bool,
    pub histogram: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Hdf5ExportMasks {
    pub pixel_masks: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Hdf5ExportAdvancedFlags {
    pub enabled: bool,
    pub shuffle: bool,
    pub hist_chunk_override: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Hdf5ExportFields {
    pub xy: bool,
    pub tot: bool,
    pub chip_id: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Hdf5ExportClusterFields {
    pub cluster_id: bool,
    pub n_hits: bool,
}

impl Default for Hdf5ExportOptions {
    fn default() -> Self {
        Self {
            datasets: Hdf5ExportDatasets {
                hits: true,
                neutrons: true,
                histogram: true,
            },
            masks: Hdf5ExportMasks { pixel_masks: true },
            advanced: Hdf5ExportAdvancedFlags {
                enabled: false,
                shuffle: true,
                hist_chunk_override: false,
            },
            compression_level: 1,
            chunk_events: 100_000,
            fields: Hdf5ExportFields {
                xy: true,
                tot: true,
                chip_id: true,
            },
            cluster_fields: Hdf5ExportClusterFields {
                cluster_id: true,
                n_hits: true,
            },
            hist_chunk_rot: 1,
            hist_chunk_y: 128,
            hist_chunk_x: 128,
            hist_chunk_tof: 64,
        }
    }
}
