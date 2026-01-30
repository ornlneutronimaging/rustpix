//! Application message types for async communication.
//!
//! Messages are sent from background worker threads to the main UI thread
//! via channels to report progress, completion, and errors.

use std::time::Duration;

use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;

/// Messages sent from background workers to the UI thread.
pub enum AppMessage {
    /// File loading progress update.
    LoadProgress(f32, String),

    /// File loading completed successfully.
    ///
    /// Contains:
    /// - `HitBatch`: Parsed detector hits
    /// - `Vec<u32>`: 512x512 pixel count grid for visualization
    /// - `Vec<u64>`: TOF histogram bins
    /// - `Duration`: Time taken to load
    /// - `String`: Debug information
    LoadComplete(Box<HitBatch>, Vec<u32>, Vec<u64>, Duration, String),

    /// File loading failed.
    LoadError(String),

    /// Clustering progress update.
    ProcessingProgress(f32, String),

    /// Clustering completed successfully.
    ///
    /// Contains:
    /// - `NeutronBatch`: Extracted neutron events
    /// - `Duration`: Time taken to process
    ProcessingComplete(NeutronBatch, Duration),

    /// Clustering failed.
    ProcessingError(String),
}
