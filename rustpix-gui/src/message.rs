//! Application message types for async communication.
//!
//! Messages are sent from background worker threads to the main UI thread
//! via channels to report progress, completion, and errors.

use std::path::PathBuf;
use std::time::Duration;

use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;

use crate::histogram::Hyperstack3D;

/// Pulse boundary metadata for cached hit batches.
#[derive(Clone, Debug)]
pub struct PulseBounds {
    /// Pulse TDC timestamp in 25ns ticks (extended across rollovers).
    pub tdc_timestamp_25ns: u64,
    /// Start index into the cached hit batch.
    pub start: usize,
    /// Number of hits in this pulse.
    pub len: usize,
}

/// Messages sent from background workers to the UI thread.
pub enum AppMessage {
    /// File loading progress update.
    LoadProgress(f32, String),

    /// File loading completed successfully.
    ///
    /// Contains:
    /// - `usize`: Total hits processed
    /// - `Option<HitBatch>`: Parsed detector hits (optional, may be skipped in streaming mode)
    /// - `Hyperstack3D`: 3D histogram data (TOF × Y × X)
    /// - `Duration`: Time taken to load
    /// - `String`: Debug information
    /// - `Option<Vec<PulseBounds>>`: Pulse boundaries for cached hits
    LoadComplete(
        usize,
        Option<Box<HitBatch>>,
        Box<Hyperstack3D>,
        Duration,
        String,
        Option<Vec<PulseBounds>>,
    ),

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

    /// Export progress update.
    ExportProgress(f32, String),

    /// Export completed successfully (path, file size bytes, validation warnings).
    ExportComplete(PathBuf, u64, Vec<String>),

    /// Export failed.
    ExportError(String),
}
