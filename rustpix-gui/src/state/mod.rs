//! Application state modules.

mod processing;
mod statistics;
mod ui;

pub use processing::ProcessingState;
pub use statistics::Statistics;
pub use ui::{SpectrumXAxis, UiState, ViewMode};
