//! UI state for panel visibility and view options.

use std::fmt;

use eframe::egui::Rect;
use egui_plot::PlotBounds;
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
    /// Cached plot bounds for ROI hit-testing before plot interaction.
    pub roi_last_plot_bounds: Option<PlotBounds>,
    /// Cached plot rect for ROI hit-testing before plot interaction.
    pub roi_last_plot_rect: Option<Rect>,
}
