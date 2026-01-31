//! UI state for panel visibility and view options.

use std::fmt;

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

/// UI panel visibility and toggle state.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct UiState {
    /// Whether to use log scale for TOF histogram Y-axis.
    pub log_plot: bool,
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
}
