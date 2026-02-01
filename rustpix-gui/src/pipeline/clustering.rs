//! Clustering worker for background processing.
//!
//! This module handles neutron clustering in a background thread,
//! processing time-ordered hit batches and extracting neutron events.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use rustpix_algorithms::{cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::neutron::NeutronBatch;
use rustpix_io::Tpx3FileReader;
use rustpix_tpx::DetectorConfig;

use super::AlgorithmType;
use crate::message::AppMessage;
use crate::util::usize_to_f32;

/// Configuration for the clustering worker.
pub struct ClusteringWorkerConfig {
    /// Spatial radius for clustering.
    pub radius: f64,
    /// Temporal window in nanoseconds.
    pub temporal_window_ns: f64,
    /// Minimum cluster size.
    pub min_cluster_size: u16,
    /// Maximum cluster size (None = unlimited).
    pub max_cluster_size: Option<u16>,
    /// Minimum points for DBSCAN.
    pub dbscan_min_points: usize,
    /// Grid cell size in pixels (Grid algorithm).
    pub grid_cell_size: usize,
    /// Detector configuration (chip layout + timing).
    pub detector_config: DetectorConfig,
    /// Super-resolution factor for extraction.
    pub super_resolution_factor: f64,
    /// Whether to weight extraction by TOT.
    pub weighted_by_tot: bool,
    /// Minimum TOT threshold for extraction.
    pub min_tot_threshold: u16,
    /// Total hits for progress calculation.
    pub total_hits: usize,
}

/// Run clustering in a background thread.
///
/// Opens the file, streams time-ordered hits, and performs clustering
/// with the specified algorithm. Progress and results are sent via the channel.
pub fn run_clustering_worker(
    path: &Path,
    tx: &Sender<AppMessage>,
    algo_type: AlgorithmType,
    config: &ClusteringWorkerConfig,
) {
    let start = Instant::now();

    let det_config = config.detector_config.clone();

    let reader = match Tpx3FileReader::open(path) {
        Ok(r) => r.with_config(det_config),
        Err(e) => {
            let _ = tx.send(AppMessage::ProcessingError(e.to_string()));
            return;
        }
    };

    let algo = match algo_type {
        AlgorithmType::Abs => ClusteringAlgorithm::Abs,
        AlgorithmType::Dbscan => ClusteringAlgorithm::Dbscan,
        AlgorithmType::Grid => ClusteringAlgorithm::Grid,
    };

    let clustering = ClusteringConfig {
        radius: config.radius,
        temporal_window_ns: config.temporal_window_ns,
        min_cluster_size: config.min_cluster_size,
        max_cluster_size: config.max_cluster_size,
    };

    let params = AlgorithmParams {
        abs_scan_interval: 100,
        dbscan_min_points: config.dbscan_min_points,
        grid_cell_size: config.grid_cell_size,
    };

    let extraction = ExtractionConfig {
        super_resolution_factor: config.super_resolution_factor,
        weighted_by_tot: config.weighted_by_tot,
        min_tot_threshold: config.min_tot_threshold,
    };

    let stream = match reader.stream_time_ordered() {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(AppMessage::ProcessingError(e.to_string()));
            return;
        }
    };

    let mut processed_hits = 0usize;
    let mut last_update = Instant::now();
    let mut neutrons = NeutronBatch::default();
    let total_hits = config.total_hits;

    for mut batch in stream {
        processed_hits = processed_hits.saturating_add(batch.len());
        let res = cluster_and_extract_batch(&mut batch, algo, &clustering, &extraction, &params);

        match res {
            Ok(n) => neutrons.append(&n),
            Err(e) => {
                let _ = tx.send(AppMessage::ProcessingError(e.to_string()));
                return;
            }
        }

        if total_hits > 0 && last_update.elapsed() > Duration::from_millis(200) {
            let progress = (usize_to_f32(processed_hits) / usize_to_f32(total_hits)).min(0.95);
            let _ = tx.send(AppMessage::ProcessingProgress(
                progress,
                format!("Processing... {:.0}%", progress * 100.0),
            ));
            last_update = Instant::now();
        }
    }

    let _ = tx.send(AppMessage::ProcessingComplete(neutrons, start.elapsed()));
}
