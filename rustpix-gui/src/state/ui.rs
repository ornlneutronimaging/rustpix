//! UI state for panel visibility and view options.

/// UI panel visibility and toggle state.
#[derive(Default)]
pub struct UiState {
    /// Whether to use log scale for TOF histogram Y-axis.
    pub log_plot: bool,
    /// Whether the TOF histogram window is visible.
    pub show_histogram: bool,
}
