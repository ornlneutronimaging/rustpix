//! UI rendering modules.
//!
//! Contains the UI rendering logic split into separate modules:
//! - `control_panel`: Left sidebar, top bar, and bottom status bar
//! - `main_view`: Central panel with histogram image, slicer, and spectrum
//! - `statistics`: Statistics display panel
//! - `theme`: Application theme and styling

mod control_panel;
mod main_view;
mod statistics;
pub mod theme;
