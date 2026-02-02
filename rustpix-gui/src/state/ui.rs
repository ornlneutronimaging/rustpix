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

#[derive(Clone, Copy, Default)]
pub struct UiPanelPopups {
    /// Whether the spectrum data selection panel is open.
    pub show_roi_panel: bool,
    /// Whether the spectrum range panel is open.
    pub show_spectrum_range: bool,
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
    /// Whether the HDF5 export dialog is open.
    pub show_dialog: bool,
    /// Whether an HDF5 export is in progress.
    pub in_progress: bool,
    /// Export progress value from 0.0 to 1.0.
    pub progress: f32,
    /// Export status message.
    pub status: String,
    /// HDF5 export configuration.
    pub options: Hdf5ExportOptions,
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
