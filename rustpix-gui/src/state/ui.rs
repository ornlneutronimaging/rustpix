//! UI state for panel visibility and view options.

/// UI panel visibility and toggle state.
#[derive(Default)]
pub struct UiState {
    /// Whether to use log scale for TOF histogram Y-axis.
    pub log_plot: bool,
    /// Whether the TOF histogram window is visible.
    pub show_histogram: bool,
    /// Whether slicer mode is enabled (show single TOF slice vs full projection).
    pub slicer_enabled: bool,
    /// Current TOF bin index for slicer view.
    pub current_tof_bin: usize,
}
