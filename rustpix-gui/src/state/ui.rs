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
#[allow(clippy::struct_excessive_bools)]
pub struct UiState {
    /// Whether the TOF histogram window is visible.
    pub show_histogram: bool,
    /// Whether slicer mode is enabled (show single TOF slice vs full projection).
    pub slicer_enabled: bool,
    /// Current TOF bin index for slicer view.
    pub current_tof_bin: usize,
    /// Current data source (Hits or Neutrons).
    pub view_mode: ViewMode,
    /// Whether to show advanced clustering parameters.
    pub show_clustering_params: bool,
    /// Whether to use log scale for X axis in spectrum.
    pub log_x: bool,
    /// Whether to use log scale for Y axis in spectrum.
    pub log_y: bool,
    /// Whether to apply log scale to the histogram view.
    pub log_scale: bool,
    /// Spectrum X-axis mode.
    pub spectrum_x_axis: SpectrumXAxis,
    /// Whether to show the app settings window.
    pub show_app_settings: bool,
    /// Whether to show the spectrum settings window.
    pub show_spectrum_settings: bool,
    /// Whether to show grid lines in the image viewer.
    pub show_grid: bool,
    /// Flag to trigger plot bounds reset (auto-fit to data).
    pub needs_plot_reset: bool,
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
    /// Whether the spectrum data selection panel is open.
    pub show_roi_panel: bool,
    /// Whether the spectrum range panel is open.
    pub show_spectrum_range: bool,
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
    /// Whether the full-FOV spectrum is visible.
    pub full_fov_visible: bool,
    /// ROI currently being renamed.
    pub roi_rename_id: Option<usize>,
    /// Editable name buffer for ROI renaming.
    pub roi_rename_text: String,
    /// Whether the HDF5 export dialog is open.
    pub show_export_dialog: bool,
    /// HDF5 export configuration.
    pub export_options: Hdf5ExportOptions,
    /// Whether to show advanced pixel health settings.
    pub show_pixel_health_settings: bool,
    /// Whether to show hot pixel overlay in the viewer.
    pub show_hot_pixels: bool,
    /// Whether to exclude masked pixels from spectra/statistics.
    pub exclude_masked_pixels: bool,
    /// Whether to cache raw hits in memory (enables rebuild/export).
    pub cache_hits_in_memory: bool,
    /// Whether an HDF5 export is in progress.
    pub export_in_progress: bool,
    /// Export progress value from 0.0 to 1.0.
    pub export_progress: f32,
    /// Export status message.
    pub export_status: String,
}

#[derive(Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Hdf5ExportOptions {
    pub include_hits: bool,
    pub include_neutrons: bool,
    pub include_histogram: bool,
    pub include_pixel_masks: bool,
    pub advanced: bool,
    pub compression_level: u8,
    pub shuffle: bool,
    pub chunk_events: usize,
    pub include_xy: bool,
    pub include_tot: bool,
    pub include_chip_id: bool,
    pub include_cluster_id: bool,
    pub include_n_hits: bool,
    pub hist_chunk_override: bool,
    pub hist_chunk_rot: usize,
    pub hist_chunk_y: usize,
    pub hist_chunk_x: usize,
    pub hist_chunk_tof: usize,
}

impl Default for Hdf5ExportOptions {
    fn default() -> Self {
        Self {
            include_hits: true,
            include_neutrons: true,
            include_histogram: true,
            include_pixel_masks: true,
            advanced: false,
            compression_level: 1,
            shuffle: true,
            chunk_events: 100_000,
            include_xy: true,
            include_tot: true,
            include_chip_id: true,
            include_cluster_id: true,
            include_n_hits: true,
            hist_chunk_override: false,
            hist_chunk_rot: 1,
            hist_chunk_y: 128,
            hist_chunk_x: 128,
            hist_chunk_tof: 64,
        }
    }
}
