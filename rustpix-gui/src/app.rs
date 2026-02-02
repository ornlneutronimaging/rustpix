//! Main application state and logic.
//!
//! Contains the `RustpixApp` struct which manages the GUI state,
//! data, and message handling.

use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use eframe::egui;
use hdf5::filters::deflate_available;
use hdf5::filters::Filter;
use hdf5::File;
use sysinfo::{get_current_pid, Pid, System};

use crate::histogram::Hyperstack3D;
use crate::message::AppMessage;
use crate::pipeline::{
    load_file_worker, run_clustering_worker, AlgorithmType, ClusteringWorkerConfig,
};
use crate::state::{Hdf5ExportOptions, ProcessingState, Statistics, UiState, ViewMode, ZoomMode};
use crate::util::{f64_to_usize_bounded, u64_to_f64, usize_to_f64};
use crate::viewer::{generate_histogram_image, Colormap, Roi, RoiShape, RoiState};
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;
use rustpix_io::hdf5::{
    write_combined_hdf5, HistogramShape, HistogramWriteData, HistogramWriteOptions,
    HitWriteOptions, NeutronEventBatch, NeutronWriteOptions, PixelMaskWriteData,
    PixelMaskWriteOptions,
};
use rustpix_io::EventBatch;
use rustpix_tpx::DetectorConfig;

#[derive(Clone)]
pub(crate) struct RoiSpectrumData {
    pub counts: Vec<u64>,
    pub area: f64,
    pub pixel_count: u64,
}

#[derive(Clone)]
pub(crate) struct PixelMaskData {
    pub width: usize,
    pub height: usize,
    pub dead_mask: Vec<u8>,
    pub hot_mask: Vec<u8>,
    pub hot_points: Vec<[f64; 2]>,
    pub dead_count: usize,
    pub hot_count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub hot_sigma: f64,
    pub hot_threshold: f64,
}

#[derive(Clone)]
pub(crate) struct RoiSpectrumEntry {
    pub data: RoiSpectrumData,
    pub shape_hash: u64,
}

#[derive(Clone, Copy)]
struct RoiSpectrumContext<'a> {
    hyperstack: &'a Hyperstack3D,
    width: usize,
    height: usize,
    n_bins: usize,
    mask: Option<&'a PixelMaskData>,
}

#[derive(Default)]
struct RoiSpectraCache {
    roi_revision: u64,
    data_revision: u64,
    spectra: HashMap<usize, RoiSpectrumEntry>,
}

struct RoiSpectrumPending {
    roi_revision: u64,
    data_revision: u64,
    last_change: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DetectorProfileKind {
    Venus,
    Custom,
}

#[derive(Clone, Debug)]
pub(crate) struct DetectorProfile {
    pub(crate) kind: DetectorProfileKind,
    pub(crate) custom_name: Option<String>,
    pub(crate) custom_path: Option<PathBuf>,
    pub(crate) custom_config: Option<DetectorConfig>,
}

impl Default for DetectorProfile {
    fn default() -> Self {
        Self {
            kind: DetectorProfileKind::Venus,
            custom_name: None,
            custom_path: None,
            custom_config: None,
        }
    }
}

impl DetectorProfile {
    pub(crate) fn label(&self) -> String {
        match self.kind {
            DetectorProfileKind::Venus => "VENUS (SNS)".to_string(),
            DetectorProfileKind::Custom => self
                .custom_name
                .clone()
                .unwrap_or_else(|| "Custom".to_string()),
        }
    }

    pub(crate) fn has_custom(&self) -> bool {
        self.custom_config.is_some()
    }
}

struct MemoryTelemetry {
    system: System,
    pid: Option<Pid>,
    last_refresh: f64,
    rss_bytes: u64,
}

impl MemoryTelemetry {
    fn new() -> Self {
        let system = System::new_all();
        let pid = get_current_pid().ok();
        Self {
            system,
            pid,
            last_refresh: -1.0,
            rss_bytes: 0,
        }
    }

    fn refresh(&mut self, now: f64) {
        const REFRESH_INTERVAL: f64 = 0.75;
        if now - self.last_refresh < REFRESH_INTERVAL {
            return;
        }
        self.last_refresh = now;
        self.system.refresh_processes();
        if let Some(pid) = self.pid {
            if let Some(process) = self.system.process(pid) {
                self.rss_bytes = process.memory();
            }
        }
    }
}

/// Main application state.
pub struct RustpixApp {
    /// Currently selected file path.
    pub(crate) selected_file: Option<PathBuf>,

    /// Selected clustering algorithm.
    pub(crate) algo_type: AlgorithmType,
    /// Clustering radius parameter.
    pub(crate) radius: f64,
    /// Temporal window in nanoseconds.
    pub(crate) temporal_window_ns: f64,
    /// Minimum cluster size.
    pub(crate) min_cluster_size: u16,
    /// Maximum cluster size (None = unlimited).
    pub(crate) max_cluster_size: Option<u16>,
    /// DBSCAN minimum points parameter.
    pub(crate) dbscan_min_points: usize,
    /// Grid cell size (pixels) for grid clustering.
    pub(crate) grid_cell_size: usize,

    /// Loaded hit batch data.
    pub(crate) hit_batch: Option<Arc<HitBatch>>,
    /// 3D hyperstack histogram (TOF × Y × X).
    pub(crate) hyperstack: Option<Arc<Hyperstack3D>>,
    /// Cached 2D projection for visualization (sum over TOF).
    pub(crate) hit_counts: Option<Vec<u64>>,
    /// Cached TOF spectrum (sum over all pixels).
    pub(crate) tof_spectrum: Option<Vec<u64>>,
    /// Cached TOF spectrum excluding masked pixels.
    pub(crate) masked_tof_spectrum: Option<Vec<u64>>,
    /// Extracted neutron events.
    pub(crate) neutrons: Arc<NeutronBatch>,
    /// 3D hyperstack for neutron data.
    pub(crate) neutron_hyperstack: Option<Arc<Hyperstack3D>>,
    /// Cached 2D projection for neutron visualization.
    pub(crate) neutron_counts: Option<Vec<u64>>,
    /// Cached TOF spectrum for neutrons.
    pub(crate) neutron_spectrum: Option<Vec<u64>>,
    /// Current cursor info (x, y, hit count).
    pub(crate) cursor_info: Option<(usize, usize, u64)>,

    /// TDC frequency in Hz.
    pub(crate) tdc_frequency: f64,
    /// Flight path length in meters (for energy conversion).
    pub(crate) flight_path_m: f64,
    /// TOF offset in nanoseconds (for energy conversion).
    pub(crate) tof_offset_ns: f64,
    /// TOF bins for hits hyperstack.
    pub(crate) hit_tof_bins: usize,
    /// TOF bins for neutron hyperstack.
    pub(crate) neutron_tof_bins: usize,
    /// Super-resolution factor for clustering extraction.
    pub(crate) super_resolution_factor: f64,
    /// Whether to weight extraction by TOT.
    pub(crate) weighted_by_tot: bool,
    /// Minimum TOT threshold for extraction.
    pub(crate) min_tot_threshold: u16,
    /// UI display state.
    pub(crate) ui_state: UiState,
    /// ROI session state.
    pub(crate) roi_state: RoiState,
    /// Cached ROI spectra for hits view.
    roi_spectra_hits: RoiSpectraCache,
    /// Cached ROI spectra for neutrons view.
    roi_spectra_neutrons: RoiSpectraCache,
    /// Pending debounce state for ROI spectrum updates.
    roi_spectrum_pending: Option<RoiSpectrumPending>,
    /// Revision counter for hit hyperstack data changes.
    pub(crate) hit_data_revision: u64,
    /// Revision counter for neutron hyperstack data changes.
    pub(crate) neutron_data_revision: u64,

    /// Message receiver for async operations.
    pub(crate) rx: Receiver<AppMessage>,
    /// Message sender for async operations.
    pub(crate) tx: Sender<AppMessage>,

    /// Processing state (loading/clustering progress).
    pub(crate) processing: ProcessingState,
    /// Session statistics.
    pub(crate) statistics: Statistics,

    /// Cached histogram texture.
    pub(crate) texture: Option<egui::TextureHandle>,
    /// Current colormap selection.
    pub(crate) colormap: Colormap,
    /// Cached dead/hot pixel masks for hits view.
    pub(crate) pixel_masks: Option<PixelMaskData>,
    /// Hot pixel sigma threshold.
    pub(crate) hot_pixel_sigma: f64,
    /// Detector configuration profile state.
    pub(crate) detector_profile: DetectorProfile,
    /// Memory telemetry for status bar display.
    memory_telemetry: MemoryTelemetry,
}

impl Default for RustpixApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        let ui_state = UiState {
            full_fov_visible: true,
            show_hot_pixels: true,
            exclude_masked_pixels: true,
            cache_hits_in_memory: true,
            ..Default::default()
        };
        Self {
            selected_file: None,
            algo_type: AlgorithmType::Abs, // Default to ABS per design doc
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            max_cluster_size: None,
            dbscan_min_points: 2,
            grid_cell_size: 32,

            hit_batch: None,
            hyperstack: None,
            hit_counts: None,
            tof_spectrum: None,
            masked_tof_spectrum: None,
            neutrons: Arc::new(NeutronBatch::default()),
            neutron_hyperstack: None,
            neutron_counts: None,
            neutron_spectrum: None,
            cursor_info: None,

            tdc_frequency: 60.0,
            flight_path_m: 0.0,
            tof_offset_ns: 0.0,
            hit_tof_bins: 200,
            neutron_tof_bins: 200,
            super_resolution_factor: 1.0,
            weighted_by_tot: false,
            min_tot_threshold: 0,
            ui_state,
            roi_state: RoiState::default(),
            roi_spectra_hits: RoiSpectraCache::default(),
            roi_spectra_neutrons: RoiSpectraCache::default(),
            roi_spectrum_pending: None,
            hit_data_revision: 0,
            neutron_data_revision: 0,
            rx,
            tx,

            processing: ProcessingState::default(),
            statistics: Statistics::default(),

            texture: None,
            colormap: Colormap::Grayscale,
            pixel_masks: None,
            hot_pixel_sigma: 5.0,
            detector_profile: DetectorProfile::default(),
            memory_telemetry: MemoryTelemetry::new(),
        }
    }
}

impl RustpixApp {
    /// Load a file asynchronously.
    pub fn load_file(&mut self, path: PathBuf) {
        self.reset_load_state(path.as_path());

        let tx = self.tx.clone();
        let detector_config = self.current_detector_config();
        let hit_tof_bins = self.hit_tof_bins;
        let cache_hits = self.ui_state.cache_hits_in_memory;
        let cancel_flag = self.processing.cancel_flag_clone();
        thread::spawn(move || {
            load_file_worker(
                path.as_path(),
                &tx,
                detector_config,
                hit_tof_bins,
                cache_hits,
                &cancel_flag,
            );
        });
    }

    /// Reset application state for a new file load.
    fn reset_load_state(&mut self, path: &Path) {
        self.selected_file = Some(path.to_path_buf());
        self.processing.is_loading = true;
        self.processing.progress = 0.0;
        self.processing.reset_cancel();
        self.processing.status_text.clear();
        self.processing.status_text.push_str("Loading file...");
        self.hit_batch = None;
        self.hyperstack = None;
        self.hit_counts = None;
        self.tof_spectrum = None;
        self.masked_tof_spectrum = None;
        self.neutrons = Arc::new(NeutronBatch::default());
        self.neutron_hyperstack = None;
        self.neutron_counts = None;
        self.neutron_spectrum = None;
        self.ui_state.view_mode = ViewMode::Hits;
        self.ui_state.full_fov_visible = true;
        self.ui_state.show_roi_panel = false;
        self.ui_state.roi_status = None;
        self.ui_state.show_spectrum_range = false;
        self.ui_state.spectrum_x_range = None;
        self.ui_state.spectrum_y_range = None;
        self.ui_state.spectrum_x_min_input.clear();
        self.ui_state.spectrum_x_max_input.clear();
        self.ui_state.spectrum_y_min_input.clear();
        self.ui_state.spectrum_y_max_input.clear();
        self.ui_state.hist_zoom_mode = ZoomMode::None;
        self.ui_state.spectrum_zoom_mode = ZoomMode::None;
        self.ui_state.hist_zoom_start = None;
        self.ui_state.spectrum_zoom_start = None;
        self.ui_state.spectrum_last_plot_bounds = None;
        self.ui_state.spectrum_last_plot_rect = None;
        self.ui_state.roi_rename_id = None;
        self.ui_state.roi_rename_text.clear();
        self.ui_state.export_in_progress = false;
        self.ui_state.export_progress = 0.0;
        self.ui_state.export_status.clear();
        self.roi_state.clear();
        self.roi_spectra_hits = RoiSpectraCache::default();
        self.roi_spectra_neutrons = RoiSpectraCache::default();
        self.roi_spectrum_pending = None;
        self.hit_data_revision = self.hit_data_revision.wrapping_add(1);
        self.neutron_data_revision = self.neutron_data_revision.wrapping_add(1);
        self.texture = None;
        self.statistics.clear();
        self.pixel_masks = None;
    }

    /// Cancel the current loading or processing operation.
    pub fn cancel_operation(&mut self) {
        self.processing.request_cancel();
        if self.processing.is_loading {
            self.processing.is_loading = false;
            self.processing.status_text = "Load cancelled".to_string();
        }
        if self.processing.is_processing {
            self.processing.is_processing = false;
            self.processing.status_text = "Processing cancelled".to_string();
        }
    }

    /// Start clustering processing asynchronously.
    pub fn run_processing(&mut self) {
        if let Some(path) = self.selected_file.clone() {
            self.processing.is_processing = true;
            self.processing.progress = 0.0;
            self.processing.status_text.clear();
            self.processing.status_text.push_str("Clustering...");

            let tx = self.tx.clone();
            let algo_type = self.algo_type;
            let config = ClusteringWorkerConfig {
                radius: self.radius,
                temporal_window_ns: self.temporal_window_ns,
                min_cluster_size: self.min_cluster_size,
                max_cluster_size: self.max_cluster_size,
                dbscan_min_points: self.dbscan_min_points,
                grid_cell_size: self.grid_cell_size,
                detector_config: self.current_detector_config(),
                super_resolution_factor: self.super_resolution_factor,
                weighted_by_tot: self.weighted_by_tot,
                min_tot_threshold: self.min_tot_threshold,
                total_hits: self
                    .hit_batch
                    .as_ref()
                    .map_or(self.statistics.hit_count, |batch| batch.len()),
                cancel_flag: self.processing.cancel_flag_clone(),
            };

            thread::spawn(move || run_clustering_worker(&path, &tx, algo_type, &config));
        }
    }

    /// Get the active hyperstack based on view mode.
    fn active_hyperstack(&self) -> Option<&Hyperstack3D> {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.hyperstack.as_deref(),
            ViewMode::Neutrons => self.neutron_hyperstack.as_deref(),
        }
    }

    /// Get the active 2D projection based on view mode.
    fn active_counts(&self) -> Option<&[u64]> {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.hit_counts.as_deref(),
            ViewMode::Neutrons => self.neutron_counts.as_deref(),
        }
    }

    /// Generate histogram image from current view (hits or neutrons).
    pub fn generate_histogram(&self) -> egui::ColorImage {
        let counts = if self.ui_state.slicer_enabled {
            // Get current TOF slice from active hyperstack
            self.active_hyperstack()
                .and_then(|hs| hs.slice_tof(self.ui_state.current_tof_bin))
        } else {
            // Full projection
            self.active_counts()
        };

        let (width, height) = self.current_dimensions();

        let Some(counts) = counts else {
            return egui::ColorImage::new([width.max(1), height.max(1)], egui::Color32::BLACK);
        };
        generate_histogram_image(
            counts,
            width,
            height,
            self.colormap,
            self.ui_state.log_scale,
        )
    }

    pub(crate) fn update_pixel_masks(&mut self) {
        let Some(counts) = self.hit_counts.as_ref() else {
            self.pixel_masks = None;
            return;
        };
        let Some(hyperstack) = self.hyperstack.as_deref() else {
            self.pixel_masks = None;
            return;
        };
        let width = hyperstack.width();
        let height = hyperstack.height();
        if counts.len() != width * height {
            self.pixel_masks = None;
            return;
        }

        let sigma = self.hot_pixel_sigma.max(0.0);
        let mut sum = 0.0f64;
        let mut sumsq = 0.0f64;
        let mut n = 0.0f64;
        for &count in counts {
            if count > 0 {
                let value = u64_to_f64(count);
                sum += value;
                sumsq += value * value;
                n += 1.0;
            }
        }

        let mean = if n > 0.0 { sum / n } else { 0.0 };
        let variance = if n > 0.0 {
            (sumsq / n) - mean * mean
        } else {
            0.0
        };
        let std_dev = variance.max(0.0).sqrt();
        let threshold = mean + sigma * std_dev;

        let mut dead_mask = Vec::with_capacity(counts.len());
        let mut hot_mask = Vec::with_capacity(counts.len());
        let mut hot_points = Vec::new();
        let mut dead_count = 0usize;
        let mut hot_count = 0usize;

        for (idx, &count) in counts.iter().enumerate() {
            if count == 0 {
                dead_mask.push(1);
                hot_mask.push(0);
                dead_count += 1;
                continue;
            }

            dead_mask.push(0);
            let is_hot = u64_to_f64(count) > threshold;
            if is_hot {
                hot_mask.push(1);
                hot_count += 1;
                let x = usize_to_f64(idx % width) + 0.5;
                let y = usize_to_f64(idx / width) + 0.5;
                hot_points.push([x, y]);
            } else {
                hot_mask.push(0);
            }
        }

        self.pixel_masks = Some(PixelMaskData {
            width,
            height,
            dead_mask,
            hot_mask,
            hot_points,
            dead_count,
            hot_count,
            mean,
            std_dev,
            hot_sigma: sigma,
            hot_threshold: threshold,
        });

        self.update_masked_spectrum();
        if self.ui_state.exclude_masked_pixels {
            self.hit_data_revision = self.hit_data_revision.wrapping_add(1);
        }
    }

    pub(crate) fn update_masked_spectrum(&mut self) {
        self.masked_tof_spectrum = None;
        if !self.ui_state.exclude_masked_pixels {
            return;
        }
        let Some(hyperstack) = self.hyperstack.as_deref() else {
            return;
        };
        let Some(mask) = self.pixel_masks.as_ref() else {
            return;
        };
        if let Some(spectrum) = compute_masked_spectrum(hyperstack, mask) {
            self.masked_tof_spectrum = Some(spectrum);
        }
    }

    fn current_detector_config(&self) -> DetectorConfig {
        let mut config = match self.detector_profile.kind {
            DetectorProfileKind::Custom => self
                .detector_profile
                .custom_config
                .clone()
                .unwrap_or_else(DetectorConfig::venus_defaults),
            DetectorProfileKind::Venus => DetectorConfig::venus_defaults(),
        };
        config.tdc_frequency_hz = self.tdc_frequency;
        config
    }

    pub(crate) fn memory_rss_bytes(&self) -> u64 {
        self.memory_telemetry.rss_bytes
    }

    pub(crate) fn memory_breakdown(&self) -> Vec<(String, u64)> {
        let mut entries = Vec::new();

        if let Some(batch) = self.hit_batch.as_deref() {
            let bytes = hit_batch_bytes(batch);
            if bytes > 0 {
                entries.push(("Hits (SoA buffers)".to_string(), bytes));
            }
        }

        if let Some(hyperstack) = self.hyperstack.as_deref() {
            let bytes = slice_bytes(hyperstack.data());
            if bytes > 0 {
                entries.push(("Hit hyperstack".to_string(), bytes));
            }
        }

        if let Some(counts) = self.hit_counts.as_ref() {
            let bytes = slice_bytes(counts.as_slice());
            if bytes > 0 {
                entries.push(("Hit projection".to_string(), bytes));
            }
        }

        if let Some(spectrum) = self.tof_spectrum.as_ref() {
            let bytes = slice_bytes(spectrum.as_slice());
            if bytes > 0 {
                entries.push(("Hit spectrum".to_string(), bytes));
            }
        }

        if let Some(spectrum) = self.masked_tof_spectrum.as_ref() {
            let bytes = slice_bytes(spectrum.as_slice());
            if bytes > 0 {
                entries.push(("Hit spectrum (masked)".to_string(), bytes));
            }
        }

        if !self.neutrons.is_empty() {
            let bytes = neutron_batch_bytes(&self.neutrons);
            if bytes > 0 {
                entries.push(("Neutrons (SoA buffers)".to_string(), bytes));
            }
        }

        if let Some(hyperstack) = self.neutron_hyperstack.as_deref() {
            let bytes = slice_bytes(hyperstack.data());
            if bytes > 0 {
                entries.push(("Neutron hyperstack".to_string(), bytes));
            }
        }

        if let Some(counts) = self.neutron_counts.as_ref() {
            let bytes = slice_bytes(counts.as_slice());
            if bytes > 0 {
                entries.push(("Neutron projection".to_string(), bytes));
            }
        }

        if let Some(spectrum) = self.neutron_spectrum.as_ref() {
            let bytes = slice_bytes(spectrum.as_slice());
            if bytes > 0 {
                entries.push(("Neutron spectrum".to_string(), bytes));
            }
        }

        let roi_hits_bytes = roi_spectra_bytes(&self.roi_spectra_hits);
        if roi_hits_bytes > 0 {
            entries.push(("ROI spectra (hits)".to_string(), roi_hits_bytes));
        }

        let roi_neutron_bytes = roi_spectra_bytes(&self.roi_spectra_neutrons);
        if roi_neutron_bytes > 0 {
            entries.push(("ROI spectra (neutrons)".to_string(), roi_neutron_bytes));
        }

        if let Some(mask) = self.pixel_masks.as_ref() {
            let mut bytes = 0u64;
            bytes += vec_len_bytes::<u8>(mask.dead_mask.len());
            bytes += vec_len_bytes::<u8>(mask.hot_mask.len());
            bytes += vec_len_bytes::<[f64; 2]>(mask.hot_points.len());
            if bytes > 0 {
                entries.push(("Pixel masks".to_string(), bytes));
            }
        }

        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries
    }

    /// Get the cached TOF spectrum (full detector integration).
    pub fn tof_spectrum(&self) -> Option<&[u64]> {
        match self.ui_state.view_mode {
            ViewMode::Hits => {
                if self.ui_state.exclude_masked_pixels {
                    self.masked_tof_spectrum
                        .as_deref()
                        .or(self.tof_spectrum.as_deref())
                } else {
                    self.tof_spectrum.as_deref()
                }
            }
            ViewMode::Neutrons => self.neutron_spectrum.as_deref(),
        }
    }

    /// Get the number of TOF bins in the active hyperstack.
    pub fn n_tof_bins(&self) -> usize {
        self.active_hyperstack()
            .map_or(0, super::histogram::Hyperstack3D::n_tof_bins)
    }

    /// Get counts for current view (projection or slice).
    pub fn current_counts(&self) -> Option<&[u64]> {
        if self.ui_state.slicer_enabled {
            self.active_hyperstack()
                .and_then(|hs| hs.slice_tof(self.ui_state.current_tof_bin))
        } else {
            self.active_counts()
        }
    }

    /// Get width/height for the active view.
    pub fn current_dimensions(&self) -> (usize, usize) {
        self.active_hyperstack()
            .map_or((512, 512), |hs| (hs.width(), hs.height()))
    }

    pub(crate) fn start_export_hdf5(&mut self, path: PathBuf) {
        if self.ui_state.export_in_progress {
            return;
        }

        let tx = self.tx.clone();
        let request = ExportHdf5Request {
            path,
            options: self.ui_state.export_options.clone(),
            hit_batch: self.hit_batch.clone(),
            neutrons: Arc::clone(&self.neutrons),
            hyperstack: self.hyperstack.clone(),
            neutron_hyperstack: self.neutron_hyperstack.clone(),
            pixel_masks: self.pixel_masks.clone(),
            view_mode: self.ui_state.view_mode,
            flight_path_m: self.flight_path_m,
            tof_offset_ns: self.tof_offset_ns,
            super_resolution_factor: self.super_resolution_factor,
        };

        self.ui_state.export_in_progress = true;
        self.ui_state.export_progress = 0.0;
        self.ui_state.export_status = "Preparing export".to_string();

        thread::spawn(move || {
            let _ = tx.send(AppMessage::ExportProgress(
                0.05,
                "Preparing export".to_string(),
            ));

            let export_path = request.path.clone();
            let result = export_hdf5_worker(&request, &tx);

            match result {
                Ok((size, warnings)) => {
                    let _ = tx.send(AppMessage::ExportComplete(export_path, size, warnings));
                }
                Err(err) => {
                    let _ = tx.send(AppMessage::ExportError(err.to_string()));
                }
            }
        });
    }

    fn build_histogram_write_data(hyperstack: &Hyperstack3D) -> HistogramWriteData {
        let width = hyperstack.width();
        let height = hyperstack.height();
        let n_bins = hyperstack.n_tof_bins();
        let data = hyperstack.data();
        let mut counts = Vec::with_capacity(width * height * n_bins);
        for y in 0..height {
            for x in 0..width {
                let base = y * width + x;
                for tof in 0..n_bins {
                    let idx = tof * height * width + base;
                    counts.push(data[idx]);
                }
            }
        }

        let rot_angle = vec![0.0];
        let y_axis = (0..height).map(usize_to_f64).collect();
        let x_axis = (0..width).map(usize_to_f64).collect();
        let bin_width_ns = hyperstack.bin_width() * 25.0;
        let time_of_flight_ns = (0..n_bins)
            .map(|i| usize_to_f64(i) * bin_width_ns)
            .collect();
        HistogramWriteData {
            counts,
            shape: HistogramShape {
                rot_angle: 1,
                y: height,
                x: width,
                time_of_flight: n_bins,
            },
            rot_angle,
            y: y_axis,
            x: x_axis,
            time_of_flight_ns,
        }
    }

    fn active_data_revision(&self) -> u64 {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.hit_data_revision,
            ViewMode::Neutrons => self.neutron_data_revision,
        }
    }

    fn active_roi_cache_mut(&mut self) -> &mut RoiSpectraCache {
        match self.ui_state.view_mode {
            ViewMode::Hits => &mut self.roi_spectra_hits,
            ViewMode::Neutrons => &mut self.roi_spectra_neutrons,
        }
    }

    fn active_roi_cache(&self) -> &RoiSpectraCache {
        match self.ui_state.view_mode {
            ViewMode::Hits => &self.roi_spectra_hits,
            ViewMode::Neutrons => &self.roi_spectra_neutrons,
        }
    }

    pub(crate) fn roi_spectrum_data(&self, roi_id: usize) -> Option<&RoiSpectrumData> {
        self.active_roi_cache()
            .spectra
            .get(&roi_id)
            .map(|entry| &entry.data)
    }

    pub(crate) fn roi_spectra_map(&self) -> &HashMap<usize, RoiSpectrumEntry> {
        &self.active_roi_cache().spectra
    }

    pub(crate) fn update_roi_spectra(&mut self, ctx: &egui::Context) {
        let roi_revision = self.roi_state.revision();
        let data_revision = self.active_data_revision();
        let needs_update = {
            let cache = self.active_roi_cache();
            cache.roi_revision != roi_revision || cache.data_revision != data_revision
        };
        if !needs_update {
            self.roi_spectrum_pending = None;
            return;
        }

        let debounce = self.roi_state.debounce_updates;
        if debounce {
            let now = ctx.input(|i| i.time);
            match self.roi_spectrum_pending.as_mut() {
                Some(pending) => {
                    if pending.roi_revision != roi_revision
                        || pending.data_revision != data_revision
                    {
                        pending.roi_revision = roi_revision;
                        pending.data_revision = data_revision;
                        pending.last_change = now;
                    }
                }
                None => {
                    self.roi_spectrum_pending = Some(RoiSpectrumPending {
                        roi_revision,
                        data_revision,
                        last_change: now,
                    });
                }
            }

            if let Some(pending) = &self.roi_spectrum_pending {
                if now - pending.last_change < 0.25 {
                    ctx.request_repaint();
                    return;
                }
            }
        }

        let start = Instant::now();
        let force_all = self.active_roi_cache().data_revision != data_revision;
        let previous = {
            let cache = self.active_roi_cache_mut();
            std::mem::take(&mut cache.spectra)
        };
        let spectra = self.compute_roi_spectra_with_cache(&previous, force_all);
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed > 0.05 {
            let expires_at = ctx.input(|i| i.time) + 1.5;
            self.ui_state.roi_status = Some(("Computing spectrum...".to_string(), expires_at));
            ctx.request_repaint();
        }
        let cache = self.active_roi_cache_mut();
        cache.spectra = spectra;
        cache.roi_revision = roi_revision;
        cache.data_revision = data_revision;
        self.roi_spectrum_pending = None;
    }

    pub(crate) fn force_roi_spectra_update(&mut self) {
        let roi_revision = self.roi_state.revision();
        let data_revision = self.active_data_revision();
        let needs_update = {
            let cache = self.active_roi_cache();
            cache.roi_revision != roi_revision || cache.data_revision != data_revision
        };
        if !needs_update {
            return;
        }
        let force_all = self.active_roi_cache().data_revision != data_revision;
        let previous = {
            let cache = self.active_roi_cache_mut();
            std::mem::take(&mut cache.spectra)
        };
        let spectra = self.compute_roi_spectra_with_cache(&previous, force_all);
        let cache = self.active_roi_cache_mut();
        cache.spectra = spectra;
        cache.roi_revision = roi_revision;
        cache.data_revision = data_revision;
        self.roi_spectrum_pending = None;
    }

    fn compute_roi_spectra_with_cache(
        &self,
        previous: &HashMap<usize, RoiSpectrumEntry>,
        force_all: bool,
    ) -> HashMap<usize, RoiSpectrumEntry> {
        let Some(hyperstack) = self.active_hyperstack() else {
            return HashMap::new();
        };
        let width = hyperstack.width();
        let height = hyperstack.height();
        let n_bins = hyperstack.n_tof_bins();
        let mask =
            if self.ui_state.exclude_masked_pixels && self.ui_state.view_mode == ViewMode::Hits {
                self.pixel_masks.as_ref()
            } else {
                None
            };
        let mut spectra = HashMap::with_capacity(self.roi_state.rois.len());

        for roi in &self.roi_state.rois {
            let shape_hash = hash_roi_shape(roi);
            if !force_all {
                if let Some(entry) = previous.get(&roi.id) {
                    if entry.shape_hash == shape_hash {
                        spectra.insert(roi.id, entry.clone());
                        continue;
                    }
                }
            }
            let Some(data) =
                Self::compute_roi_spectrum(roi, hyperstack, width, height, n_bins, mask)
            else {
                continue;
            };
            spectra.insert(roi.id, RoiSpectrumEntry { data, shape_hash });
        }

        spectra
    }

    fn compute_roi_spectrum(
        roi: &Roi,
        hyperstack: &Hyperstack3D,
        width: usize,
        height: usize,
        n_bins: usize,
        mask: Option<&PixelMaskData>,
    ) -> Option<RoiSpectrumData> {
        let mask = Self::active_pixel_mask(mask, width, height);
        let ctx = RoiSpectrumContext {
            hyperstack,
            width,
            height,
            n_bins,
            mask,
        };
        match &roi.shape {
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                Some(Self::compute_rect_spectrum(*x1, *y1, *x2, *y2, ctx))
            }
            RoiShape::Polygon { vertices } => {
                Self::compute_polygon_spectrum(vertices, hyperstack, width, height, n_bins, mask)
            }
        }
    }

    fn active_pixel_mask(
        mask: Option<&PixelMaskData>,
        width: usize,
        height: usize,
    ) -> Option<&PixelMaskData> {
        mask.filter(|m| m.dead_mask.len() == width * height && m.hot_mask.len() == width * height)
    }

    fn sum_counts_for_indices(
        hyperstack: &Hyperstack3D,
        n_bins: usize,
        indices: &[usize],
    ) -> Vec<u64> {
        let mut totals = vec![0u64; n_bins];
        for (tof_bin, total) in totals.iter_mut().enumerate() {
            let Some(slice) = hyperstack.slice_tof(tof_bin) else {
                continue;
            };
            let mut sum = 0u64;
            for idx in indices {
                sum += slice[*idx];
            }
            *total = sum;
        }
        totals
    }

    fn collect_rect_indices(
        width: usize,
        x_start: usize,
        x_end: usize,
        y_start: usize,
        y_end: usize,
        mask: Option<&PixelMaskData>,
    ) -> Vec<usize> {
        let mut indices = Vec::new();
        for y in y_start..y_end {
            let row = y * width;
            for x in x_start..x_end {
                let idx = row + x;
                if let Some(mask) = mask {
                    if mask.dead_mask[idx] == 1 || mask.hot_mask[idx] == 1 {
                        continue;
                    }
                }
                indices.push(idx);
            }
        }
        indices
    }

    fn compute_rect_spectrum(
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        ctx: RoiSpectrumContext<'_>,
    ) -> RoiSpectrumData {
        let (x_start, x_end) = clamp_span(x1, x2, ctx.width);
        let (y_start, y_end) = clamp_span(y1, y2, ctx.height);
        let area = (x2 - x1).abs() * (y2 - y1).abs();

        if x_start >= x_end || y_start >= y_end {
            return RoiSpectrumData {
                counts: vec![0; ctx.n_bins],
                area,
                pixel_count: 0,
            };
        }

        if let Some(mask) = ctx.mask {
            let indices =
                Self::collect_rect_indices(ctx.width, x_start, x_end, y_start, y_end, Some(mask));
            let counts = Self::sum_counts_for_indices(ctx.hyperstack, ctx.n_bins, &indices);
            let pixel_count = indices.len() as u64;
            return RoiSpectrumData {
                counts,
                area,
                pixel_count,
            };
        }

        let counts = ctx.hyperstack.spectrum(x_start..x_end, y_start..y_end);
        let pixel_count = (x_end - x_start) as u64 * (y_end - y_start) as u64;
        RoiSpectrumData {
            counts,
            area,
            pixel_count,
        }
    }

    fn compute_polygon_spectrum(
        vertices: &[(f64, f64)],
        hyperstack: &Hyperstack3D,
        width: usize,
        height: usize,
        n_bins: usize,
        mask: Option<&PixelMaskData>,
    ) -> Option<RoiSpectrumData> {
        if vertices.len() < 3 {
            return None;
        }
        let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
        for (x, y) in vertices {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
        let (x_start, x_end) = clamp_span(min_x, max_x, width);
        let (y_start, y_end) = clamp_span(min_y, max_y, height);
        let mut roi_indices = Vec::new();
        for y in y_start..y_end {
            let py = usize_to_f64(y) + 0.5;
            for x in x_start..x_end {
                let px = usize_to_f64(x) + 0.5;
                if point_in_polygon_xy(px, py, vertices) {
                    if let Some(mask) = mask {
                        let idx = y * width + x;
                        if mask.dead_mask[idx] == 1 || mask.hot_mask[idx] == 1 {
                            continue;
                        }
                    }
                    roi_indices.push(y * width + x);
                }
            }
        }
        let counts = Self::sum_counts_for_indices(hyperstack, n_bins, &roi_indices);
        let area = polygon_area(vertices).abs();
        Some(RoiSpectrumData {
            counts,
            area,
            pixel_count: roi_indices.len() as u64,
        })
    }

    /// Check if neutron data is available.
    pub fn has_neutrons(&self) -> bool {
        self.neutron_hyperstack.is_some()
    }

    /// Rebuild the hits hyperstack with current settings.
    pub fn rebuild_hit_hyperstack(&mut self) {
        let Some(hit_batch) = self.hit_batch.as_deref() else {
            return;
        };
        let tof_max = self.statistics.tof_max;
        let bins = self.hit_tof_bins.max(1);
        let hyperstack = Hyperstack3D::from_hits(hit_batch, bins, tof_max, 512, 512);
        self.hit_counts = Some(hyperstack.project_xy());
        self.tof_spectrum = Some(hyperstack.full_spectrum());
        self.hyperstack = Some(Arc::new(hyperstack));
        self.update_pixel_masks();
        self.hit_data_revision = self.hit_data_revision.wrapping_add(1);
        self.ui_state.current_tof_bin = 0;
        self.ui_state.needs_plot_reset = true;
        self.texture = None;
    }

    /// Rebuild the neutron hyperstack with current settings.
    pub fn rebuild_neutron_hyperstack(&mut self) {
        if self.neutrons.is_empty() {
            return;
        }
        let tof_max = self.statistics.tof_max;
        let bins = self.neutron_tof_bins.max(1);
        let neutron_hs = Hyperstack3D::from_neutrons(
            &self.neutrons,
            bins,
            tof_max,
            512,
            512,
            self.super_resolution_factor,
        );
        self.neutron_counts = Some(neutron_hs.project_xy());
        self.neutron_spectrum = Some(neutron_hs.full_spectrum());
        self.neutron_hyperstack = Some(Arc::new(neutron_hs));
        self.neutron_data_revision = self.neutron_data_revision.wrapping_add(1);
        self.ui_state.current_tof_bin = 0;
        self.ui_state.needs_plot_reset = true;
        self.texture = None;
    }

    /// Handle pending messages from async workers.
    pub fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::LoadProgress(p, s) => self.handle_load_progress(p, s),
                AppMessage::ProcessingProgress(p, s) => self.handle_processing_progress(p, s),
                AppMessage::LoadComplete(hit_count, batch, hyperstack, dur, _dbg) => {
                    self.handle_load_complete(ctx, hit_count, batch, *hyperstack, dur);
                }
                AppMessage::LoadError(e) => self.handle_load_error(&e),
                AppMessage::ProcessingComplete(neutrons, dur) => {
                    self.handle_processing_complete(neutrons, dur);
                }
                AppMessage::ProcessingError(e) => self.handle_processing_error(&e),
                AppMessage::ExportProgress(progress, status) => {
                    self.handle_export_progress(progress, status);
                }
                AppMessage::ExportComplete(path, size_bytes, warnings) => {
                    self.handle_export_complete(ctx, &path, size_bytes, &warnings);
                }
                AppMessage::ExportError(e) => self.handle_export_error(ctx, &e),
            }
        }
    }

    fn handle_load_progress(&mut self, progress: f32, status: String) {
        if self.processing.is_loading {
            self.processing.progress = progress;
            self.processing.status_text = status;
        }
    }

    fn handle_processing_progress(&mut self, progress: f32, status: String) {
        if self.processing.is_processing {
            self.processing.progress = progress;
            self.processing.status_text = status;
        }
    }

    fn handle_load_complete(
        &mut self,
        ctx: &egui::Context,
        hit_count: usize,
        batch: Option<Box<HitBatch>>,
        hyperstack: Hyperstack3D,
        dur: Duration,
    ) {
        if !self.processing.is_loading {
            return;
        }
        self.processing.is_loading = false;
        self.processing.progress = 1.0;
        self.processing.status_text = "Ready".to_string();

        self.statistics.hit_count = hit_count;
        self.statistics.load_duration = Some(dur);
        self.statistics.tof_max = hyperstack.tof_max();

        self.hit_counts = Some(hyperstack.project_xy());
        self.tof_spectrum = Some(hyperstack.full_spectrum());
        self.hyperstack = Some(Arc::new(hyperstack));
        self.hit_batch = batch.map(|batch| Arc::new(*batch));
        self.update_pixel_masks();
        self.hit_data_revision = self.hit_data_revision.wrapping_add(1);

        self.ui_state.needs_plot_reset = true;

        let img = self.generate_histogram();
        self.texture = Some(ctx.load_texture("hist", img, egui::TextureOptions::NEAREST));
    }

    fn handle_load_error(&mut self, error: &str) {
        self.processing.is_loading = false;
        self.processing.status_text = format!("Error: {error}");
    }

    fn handle_processing_complete(&mut self, neutrons: NeutronBatch, dur: Duration) {
        if !self.processing.is_processing {
            return;
        }
        self.processing.is_processing = false;
        self.processing.progress = 1.0;
        self.processing.status_text = "Ready".to_string();

        self.statistics.neutron_count = neutrons.len();
        self.statistics.cluster_duration = Some(dur);
        if !neutrons.is_empty() && self.statistics.hit_count > 0 {
            #[allow(clippy::cast_precision_loss)]
            {
                self.statistics.avg_cluster_size =
                    self.statistics.hit_count as f64 / neutrons.len() as f64;
            }
        }

        if let Some(hit_hs) = self.hyperstack.as_deref() {
            let neutron_hs = Hyperstack3D::from_neutrons(
                &neutrons,
                self.neutron_tof_bins.max(1),
                hit_hs.tof_max(),
                hit_hs.width(),
                hit_hs.height(),
                self.super_resolution_factor,
            );
            self.neutron_counts = Some(neutron_hs.project_xy());
            self.neutron_spectrum = Some(neutron_hs.full_spectrum());
            self.neutron_hyperstack = Some(Arc::new(neutron_hs));
            self.neutron_data_revision = self.neutron_data_revision.wrapping_add(1);
        }

        self.neutrons = Arc::new(neutrons);
    }

    fn handle_processing_error(&mut self, error: &str) {
        self.processing.is_processing = false;
        self.processing.status_text = format!("Error: {error}");
    }

    fn handle_export_progress(&mut self, progress: f32, status: String) {
        self.ui_state.export_in_progress = true;
        self.ui_state.export_progress = progress;
        self.ui_state.export_status = status;
    }

    fn handle_export_complete(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        size_bytes: u64,
        warnings: &[String],
    ) {
        self.ui_state.export_in_progress = false;
        self.ui_state.export_progress = 1.0;
        self.ui_state.export_status = if warnings.is_empty() {
            "Export complete".to_string()
        } else {
            "Export complete (warnings)".to_string()
        };
        let size_mb = u64_to_f64(size_bytes) / (1024.0 * 1024.0);
        self.ui_state.roi_status = Some((
            format!("Saved HDF5: {} ({size_mb:.1} MB)", path.display()),
            ctx.input(|i| i.time + 4.0),
        ));
        if !warnings.is_empty() {
            log::warn!("HDF5 export validation warnings:");
            for warning in warnings {
                log::warn!(" - {warning}");
            }
            let summary = if warnings.len() == 1 {
                warnings[0].clone()
            } else {
                format!("{} (and {} more)", warnings[0], warnings.len() - 1)
            };
            self.ui_state.roi_warning = Some((
                format!("Export validation: {summary}"),
                ctx.input(|i| i.time + 6.0),
            ));
        }
    }

    fn handle_export_error(&mut self, ctx: &egui::Context, error: &str) {
        self.ui_state.export_in_progress = false;
        self.ui_state.export_status = "Export failed".to_string();
        self.ui_state.roi_warning = Some((
            format!("HDF5 export failed: {error}"),
            ctx.input(|i| i.time + 6.0),
        ));
    }
}

struct ExportHdf5Request {
    path: PathBuf,
    options: Hdf5ExportOptions,
    hit_batch: Option<Arc<HitBatch>>,
    neutrons: Arc<NeutronBatch>,
    hyperstack: Option<Arc<Hyperstack3D>>,
    neutron_hyperstack: Option<Arc<Hyperstack3D>>,
    pixel_masks: Option<PixelMaskData>,
    view_mode: ViewMode,
    flight_path_m: f64,
    tof_offset_ns: f64,
    super_resolution_factor: f64,
}

fn export_hdf5_worker(
    request: &ExportHdf5Request,
    tx: &Sender<AppMessage>,
) -> Result<(u64, Vec<String>)> {
    ensure_deflate_available()?;

    let (hit_payload, hit_options) = prepare_hit_export(request, tx)?;
    let (neutron_payload, neutron_options) = prepare_neutron_export(request, tx)?;
    let (histogram_payload, histogram_options) = prepare_histogram_export(request, tx)?;
    let (mask_payload, mask_options) = prepare_mask_export(request, tx)?;

    ensure_export_selection(
        hit_payload.as_ref(),
        neutron_payload.as_ref(),
        histogram_payload.as_ref(),
        mask_payload.as_ref(),
    )?;

    send_export_progress(tx, 0.85, "Writing HDF5");
    write_combined_hdf5(
        &request.path,
        hit_payload.as_ref().zip(hit_options.as_ref()),
        neutron_payload.as_ref().zip(neutron_options.as_ref()),
        histogram_payload.as_ref().zip(histogram_options.as_ref()),
        mask_payload.as_ref().zip(mask_options.as_ref()),
    )
    .map_err(|err| {
        remove_partial_file(&request.path);
        anyhow!("HDF5 export failed: {err}")
    })?;

    send_export_progress(tx, 0.95, "Validating export");
    let warnings = match validate_hdf5_export(&request.path, &request.options) {
        Ok(list) => list,
        Err(err) => vec![format!("Validation failed: {err}")],
    };

    let size = std::fs::metadata(&request.path)
        .map(|m| m.len())
        .unwrap_or(0);
    send_export_progress(tx, 1.0, "Export complete");
    Ok((size, warnings))
}

fn send_export_progress(tx: &Sender<AppMessage>, progress: f32, status: &str) {
    let _ = tx.send(AppMessage::ExportProgress(progress, status.to_string()));
}

fn prepare_hit_export(
    request: &ExportHdf5Request,
    tx: &Sender<AppMessage>,
) -> Result<(Option<EventBatch>, Option<HitWriteOptions>)> {
    if !request.options.include_hits {
        return Ok((None, None));
    }
    send_export_progress(tx, 0.15, "Preparing hits");
    let batch = request
        .hit_batch
        .as_deref()
        .ok_or_else(|| anyhow!("No hits loaded"))?;
    if batch.is_empty() {
        return Err(anyhow!("No hits to export"));
    }
    let (x_size, y_size) = hit_dimensions_from(request.hyperstack.as_deref(), Some(batch))
        .ok_or_else(|| anyhow!("Unable to determine detector dimensions"))?;
    let compression_level = request.options.compression_level.min(9);
    let options = HitWriteOptions {
        x_size,
        y_size,
        chunk_events: request.options.chunk_events.max(1),
        compression: Some(compression_level),
        shuffle: request.options.shuffle,
        flight_path_m: optional_positive(request.flight_path_m),
        tof_offset_ns: optional_nonzero(request.tof_offset_ns),
        energy_axis_kind: Some("tof".to_string()),
        include_xy: request.options.include_xy,
        include_tot: request.options.include_tot,
        include_chip_id: request.options.include_chip_id,
        include_cluster_id: request.options.include_cluster_id,
    };
    let payload = EventBatch {
        tdc_timestamp_25ns: 0,
        hits: (*batch).clone(),
    };
    Ok((Some(payload), Some(options)))
}

fn prepare_neutron_export(
    request: &ExportHdf5Request,
    tx: &Sender<AppMessage>,
) -> Result<(Option<NeutronEventBatch>, Option<NeutronWriteOptions>)> {
    if !request.options.include_neutrons {
        return Ok((None, None));
    }
    send_export_progress(tx, 0.35, "Preparing neutrons");
    if request.neutrons.is_empty() {
        return Err(anyhow!("No neutrons available"));
    }
    let scaled = scale_neutrons_for_export(&request.neutrons, request.super_resolution_factor)?;
    let (x_size, y_size) = neutron_dimensions_from(request.neutron_hyperstack.as_deref(), &scaled)
        .ok_or_else(|| anyhow!("Unable to determine detector dimensions"))?;
    let compression_level = request.options.compression_level.min(9);
    let options = NeutronWriteOptions {
        x_size,
        y_size,
        chunk_events: request.options.chunk_events.max(1),
        compression: Some(compression_level),
        shuffle: request.options.shuffle,
        flight_path_m: optional_positive(request.flight_path_m),
        tof_offset_ns: optional_nonzero(request.tof_offset_ns),
        energy_axis_kind: Some("tof".to_string()),
        include_xy: request.options.include_xy,
        include_tot: request.options.include_tot,
        include_chip_id: request.options.include_chip_id,
        include_n_hits: request.options.include_n_hits,
    };
    let payload = NeutronEventBatch {
        tdc_timestamp_25ns: 0,
        neutrons: scaled,
    };
    Ok((Some(payload), Some(options)))
}

fn prepare_histogram_export(
    request: &ExportHdf5Request,
    tx: &Sender<AppMessage>,
) -> Result<(Option<HistogramWriteData>, Option<HistogramWriteOptions>)> {
    if !request.options.include_histogram {
        return Ok((None, None));
    }
    send_export_progress(tx, 0.55, "Preparing histogram");
    let hyper = match request.view_mode {
        ViewMode::Hits => request.hyperstack.as_deref(),
        ViewMode::Neutrons => request.neutron_hyperstack.as_deref(),
    }
    .ok_or_else(|| anyhow!("No histogram data available"))?;
    let width = hyper.width();
    let height = hyper.height();
    let n_bins = hyper.n_tof_bins();
    let payload = RustpixApp::build_histogram_write_data(hyper);
    let compression_level = request.options.compression_level.min(9);
    let mut options = HistogramWriteOptions {
        compression: Some(compression_level),
        shuffle: request.options.shuffle,
        flight_path_m: optional_positive(request.flight_path_m),
        tof_offset_ns: optional_nonzero(request.tof_offset_ns),
        ..Default::default()
    };
    if request.options.hist_chunk_override {
        options.chunk_counts = Some([
            request.options.hist_chunk_rot.clamp(1, 1),
            request.options.hist_chunk_y.clamp(1, height),
            request.options.hist_chunk_x.clamp(1, width),
            request.options.hist_chunk_tof.clamp(1, n_bins),
        ]);
    }
    Ok((Some(payload), Some(options)))
}

fn prepare_mask_export(
    request: &ExportHdf5Request,
    tx: &Sender<AppMessage>,
) -> Result<(Option<PixelMaskWriteData>, Option<PixelMaskWriteOptions>)> {
    if !request.options.include_pixel_masks {
        return Ok((None, None));
    }
    send_export_progress(tx, 0.7, "Preparing pixel masks");
    let masks = request
        .pixel_masks
        .as_ref()
        .ok_or_else(|| anyhow!("No pixel masks available"))?;
    let compression_level = request.options.compression_level.min(9);
    let payload = PixelMaskWriteData {
        width: masks.width,
        height: masks.height,
        dead_mask: masks.dead_mask.clone(),
        hot_mask: masks.hot_mask.clone(),
        hot_sigma: masks.hot_sigma,
        hot_threshold: masks.hot_threshold,
        mean: masks.mean,
        std_dev: masks.std_dev,
    };
    let options = PixelMaskWriteOptions {
        compression: Some(compression_level),
        shuffle: request.options.shuffle,
    };
    Ok((Some(payload), Some(options)))
}

fn ensure_export_selection(
    hit_payload: Option<&EventBatch>,
    neutron_payload: Option<&NeutronEventBatch>,
    histogram_payload: Option<&HistogramWriteData>,
    mask_payload: Option<&PixelMaskWriteData>,
) -> Result<()> {
    if hit_payload.is_none()
        && neutron_payload.is_none()
        && histogram_payload.is_none()
        && mask_payload.is_none()
    {
        Err(anyhow!("No data selected for export"))
    } else {
        Ok(())
    }
}

fn optional_positive(value: f64) -> Option<f64> {
    if value > 0.0 {
        Some(value)
    } else {
        None
    }
}

fn optional_nonzero(value: f64) -> Option<f64> {
    if value == 0.0 {
        None
    } else {
        Some(value)
    }
}

fn scale_neutrons_for_export(
    neutrons: &NeutronBatch,
    super_resolution_factor: f64,
) -> Result<NeutronBatch> {
    let count = neutrons.len();
    if count == 0 {
        return Err(anyhow!("No neutrons available"));
    }
    let factor = if super_resolution_factor.is_finite() && super_resolution_factor > 0.0 {
        super_resolution_factor
    } else {
        1.0
    };
    let mut batch = NeutronBatch::with_capacity(count);
    for i in 0..count {
        batch.x.push(neutrons.x[i] / factor);
        batch.y.push(neutrons.y[i] / factor);
        batch.tof.push(neutrons.tof[i]);
        batch.tot.push(neutrons.tot[i]);
        batch.n_hits.push(neutrons.n_hits[i]);
        batch.chip_id.push(neutrons.chip_id[i]);
    }
    Ok(batch)
}

fn ensure_deflate_available() -> Result<()> {
    if deflate_available() {
        Ok(())
    } else {
        Err(anyhow!(
            "HDF5 deflate (zlib) filter unavailable; rebuild with hdf5 zlib support"
        ))
    }
}

fn remove_partial_file(path: &Path) {
    if let Err(err) = std::fs::remove_file(path) {
        log::warn!(
            "Failed to remove partial HDF5 file {}: {err}",
            path.display()
        );
    }
}

fn validate_hdf5_export(path: &Path, options: &Hdf5ExportOptions) -> Result<Vec<String>> {
    let file = File::open(path)?;
    let entry = file
        .group("entry")
        .map_err(|err| anyhow!("Missing entry group: {err}"))?;
    let mut warnings = Vec::new();

    if options.include_hits {
        validate_hits_group(&entry, options, &mut warnings);
    }
    if options.include_neutrons {
        validate_neutrons_group(&entry, options, &mut warnings);
    }
    if options.include_histogram {
        validate_histogram_group(&entry, options, &mut warnings);
    }
    if options.include_pixel_masks {
        validate_pixel_mask_group(&entry, options, &mut warnings);
    }

    Ok(warnings)
}

fn validate_hits_group(
    entry: &hdf5::Group,
    options: &Hdf5ExportOptions,
    warnings: &mut Vec<String>,
) {
    let Ok(group) = entry.group("hits") else {
        warnings.push("Missing group: entry/hits".to_string());
        return;
    };
    let required = [
        "event_id",
        "event_time_offset",
        "event_time_zero",
        "event_index",
    ];
    for name in required {
        if group.dataset(name).is_err() {
            warnings.push(format!("Missing hits dataset: {name}"));
        }
    }
    if options.include_tot && group.dataset("time_over_threshold").is_err() {
        warnings.push("Missing hits dataset: time_over_threshold".to_string());
    }
    if options.include_chip_id && group.dataset("chip_id").is_err() {
        warnings.push("Missing hits dataset: chip_id".to_string());
    }
    if options.include_cluster_id && group.dataset("cluster_id").is_err() {
        warnings.push("Missing hits dataset: cluster_id".to_string());
    }
    if options.include_xy {
        if group.dataset("x").is_err() {
            warnings.push("Missing hits dataset: x".to_string());
        }
        if group.dataset("y").is_err() {
            warnings.push("Missing hits dataset: y".to_string());
        }
    }

    if let Ok(event_id) = group.dataset("event_id") {
        if event_id.size() == 0 {
            warnings.push("Hits dataset is empty".to_string());
        }
        check_dataset_filters(&event_id, "hits/event_id", options, warnings);
    }
    if let Ok(time_offset) = group.dataset("event_time_offset") {
        check_dataset_filters(&time_offset, "hits/event_time_offset", options, warnings);
    }
}

fn validate_neutrons_group(
    entry: &hdf5::Group,
    options: &Hdf5ExportOptions,
    warnings: &mut Vec<String>,
) {
    let Ok(group) = entry.group("neutrons") else {
        warnings.push("Missing group: entry/neutrons".to_string());
        return;
    };
    let required = [
        "event_id",
        "event_time_offset",
        "event_time_zero",
        "event_index",
    ];
    for name in required {
        if group.dataset(name).is_err() {
            warnings.push(format!("Missing neutrons dataset: {name}"));
        }
    }
    if options.include_tot && group.dataset("time_over_threshold").is_err() {
        warnings.push("Missing neutrons dataset: time_over_threshold".to_string());
    }
    if options.include_chip_id && group.dataset("chip_id").is_err() {
        warnings.push("Missing neutrons dataset: chip_id".to_string());
    }
    if options.include_n_hits && group.dataset("n_hits").is_err() {
        warnings.push("Missing neutrons dataset: n_hits".to_string());
    }
    if options.include_xy {
        if group.dataset("x").is_err() {
            warnings.push("Missing neutrons dataset: x".to_string());
        }
        if group.dataset("y").is_err() {
            warnings.push("Missing neutrons dataset: y".to_string());
        }
    }

    if let Ok(event_id) = group.dataset("event_id") {
        if event_id.size() == 0 {
            warnings.push("Neutron dataset is empty".to_string());
        }
        check_dataset_filters(&event_id, "neutrons/event_id", options, warnings);
    }
    if let Ok(time_offset) = group.dataset("event_time_offset") {
        check_dataset_filters(
            &time_offset,
            "neutrons/event_time_offset",
            options,
            warnings,
        );
    }
}

fn validate_histogram_group(
    entry: &hdf5::Group,
    options: &Hdf5ExportOptions,
    warnings: &mut Vec<String>,
) {
    let Ok(group) = entry.group("histogram") else {
        warnings.push("Missing group: entry/histogram".to_string());
        return;
    };
    let Ok(counts) = group.dataset("counts") else {
        warnings.push("Missing histogram dataset: counts".to_string());
        return;
    };
    if counts.size() == 0 {
        warnings.push("Histogram counts dataset is empty".to_string());
    }
    check_dataset_filters(&counts, "histogram/counts", options, warnings);
}

fn validate_pixel_mask_group(
    entry: &hdf5::Group,
    options: &Hdf5ExportOptions,
    warnings: &mut Vec<String>,
) {
    let Ok(group) = entry.group("pixel_masks") else {
        warnings.push("Missing group: entry/pixel_masks".to_string());
        return;
    };
    if let Ok(dead) = group.dataset("dead") {
        if dead.size() == 0 {
            warnings.push("Pixel mask dataset dead is empty".to_string());
        }
        check_dataset_filters(&dead, "pixel_masks/dead", options, warnings);
    } else {
        warnings.push("Missing pixel mask dataset: dead".to_string());
    }
    if let Ok(hot) = group.dataset("hot") {
        if hot.size() == 0 {
            warnings.push("Pixel mask dataset hot is empty".to_string());
        }
        check_dataset_filters(&hot, "pixel_masks/hot", options, warnings);
    } else {
        warnings.push("Missing pixel mask dataset: hot".to_string());
    }
}

fn check_dataset_filters(
    dataset: &hdf5::Dataset,
    label: &str,
    options: &Hdf5ExportOptions,
    warnings: &mut Vec<String>,
) {
    let filters = dataset.filters();
    if options.compression_level > 0 {
        let has_deflate = filters.iter().any(|f| matches!(f, Filter::Deflate(_)));
        if !has_deflate {
            warnings.push(format!("Missing deflate compression on {label}"));
        }
    }
    if options.shuffle {
        let has_shuffle = filters.iter().any(|f| matches!(f, Filter::Shuffle));
        if !has_shuffle {
            warnings.push(format!("Missing shuffle filter on {label}"));
        }
    }
}

fn f64_to_u32_checked(value: f64) -> Option<u32> {
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if rounded < 0.0 || rounded > f64::from(u32::MAX) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        Some(rounded as u32)
    }
}

fn hit_dimensions_from(
    hyperstack: Option<&Hyperstack3D>,
    batch: Option<&HitBatch>,
) -> Option<(u32, u32)> {
    if let Some(hs) = hyperstack {
        let width = u32::try_from(hs.width()).ok()?;
        let height = u32::try_from(hs.height()).ok()?;
        return Some((width, height));
    }
    let batch = batch?;
    let max_x = batch.x.iter().copied().max().unwrap_or(0);
    let max_y = batch.y.iter().copied().max().unwrap_or(0);
    Some((u32::from(max_x) + 1, u32::from(max_y) + 1))
}

fn neutron_dimensions_from(
    hyperstack: Option<&Hyperstack3D>,
    neutrons: &NeutronBatch,
) -> Option<(u32, u32)> {
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut saw_point = false;
    for (&x, &y) in neutrons.x.iter().zip(neutrons.y.iter()) {
        let x_u32 = f64_to_u32_checked(x)?;
        let y_u32 = f64_to_u32_checked(y)?;
        saw_point = true;
        if x_u32 > max_x {
            max_x = x_u32;
        }
        if y_u32 > max_y {
            max_y = y_u32;
        }
    }
    if !saw_point {
        return None;
    }
    let mut x_size = max_x.saturating_add(1);
    let mut y_size = max_y.saturating_add(1);
    if let Some(hs) = hyperstack {
        if let Ok(width) = u32::try_from(hs.width()) {
            x_size = x_size.max(width);
        }
        if let Ok(height) = u32::try_from(hs.height()) {
            y_size = y_size.max(height);
        }
    }
    Some((x_size, y_size))
}

fn hash_roi_shape(roi: &Roi) -> u64 {
    let mut hasher = DefaultHasher::new();
    match &roi.shape {
        RoiShape::Rectangle { x1, y1, x2, y2 } => {
            0u8.hash(&mut hasher);
            hash_f64(*x1, &mut hasher);
            hash_f64(*y1, &mut hasher);
            hash_f64(*x2, &mut hasher);
            hash_f64(*y2, &mut hasher);
        }
        RoiShape::Polygon { vertices } => {
            1u8.hash(&mut hasher);
            vertices.len().hash(&mut hasher);
            for (x, y) in vertices {
                hash_f64(*x, &mut hasher);
                hash_f64(*y, &mut hasher);
            }
        }
    }
    hasher.finish()
}

fn hash_f64(value: f64, hasher: &mut DefaultHasher) {
    value.to_bits().hash(hasher);
}

fn clamp_span(a: f64, b: f64, limit: usize) -> (usize, usize) {
    let min = a.min(b);
    let max = a.max(b);
    let max_f64 = usize_to_f64(limit);
    let start_f64 = min.floor().clamp(0.0, max_f64);
    let end_f64 = max.ceil().clamp(0.0, max_f64);
    let max_exclusive = limit.saturating_add(1);
    let start = f64_to_usize_bounded(start_f64, max_exclusive).unwrap_or(limit);
    let end = f64_to_usize_bounded(end_f64, max_exclusive).unwrap_or(limit);
    (start, end)
}

fn polygon_area(vertices: &[(f64, f64)]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut area2 = 0.0;
    for i in 0..vertices.len() {
        let (x0, y0) = vertices[i];
        let (x1, y1) = vertices[(i + 1) % vertices.len()];
        area2 += (x0 * y1) - (x1 * y0);
    }
    area2 * 0.5
}

fn point_in_polygon_xy(x: f64, y: f64, vertices: &[(f64, f64)]) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = vertices.len() - 1;
    for i in 0..vertices.len() {
        let (xi, yi) = vertices[i];
        let (xj, yj) = vertices[j];
        let intersects = ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi);
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn compute_masked_spectrum(hyperstack: &Hyperstack3D, mask: &PixelMaskData) -> Option<Vec<u64>> {
    let width = hyperstack.width();
    let height = hyperstack.height();
    let expected = width.saturating_mul(height);
    if mask.dead_mask.len() != expected || mask.hot_mask.len() != expected {
        return None;
    }
    let mut active_indices = Vec::new();
    for idx in 0..expected {
        if mask.dead_mask[idx] == 0 && mask.hot_mask[idx] == 0 {
            active_indices.push(idx);
        }
    }
    let n_bins = hyperstack.n_tof_bins();
    let mut counts = vec![0u64; n_bins];
    for (tof_bin, total) in counts.iter_mut().enumerate() {
        let Some(slice) = hyperstack.slice_tof(tof_bin) else {
            continue;
        };
        let mut sum = 0u64;
        for idx in &active_indices {
            sum += slice[*idx];
        }
        *total = sum;
    }
    Some(counts)
}

fn vec_len_bytes<T>(len: usize) -> u64 {
    (len as u64).saturating_mul(size_of::<T>() as u64)
}

fn slice_bytes<T>(slice: &[T]) -> u64 {
    vec_len_bytes::<T>(slice.len())
}

fn vec_capacity_bytes<T>(vec: &Vec<T>) -> u64 {
    (vec.capacity() as u64).saturating_mul(size_of::<T>() as u64)
}

fn hit_batch_bytes(batch: &HitBatch) -> u64 {
    vec_capacity_bytes(&batch.x)
        + vec_capacity_bytes(&batch.y)
        + vec_capacity_bytes(&batch.tof)
        + vec_capacity_bytes(&batch.tot)
        + vec_capacity_bytes(&batch.timestamp)
        + vec_capacity_bytes(&batch.chip_id)
        + vec_capacity_bytes(&batch.cluster_id)
}

fn neutron_batch_bytes(batch: &NeutronBatch) -> u64 {
    vec_capacity_bytes(&batch.x)
        + vec_capacity_bytes(&batch.y)
        + vec_capacity_bytes(&batch.tof)
        + vec_capacity_bytes(&batch.tot)
        + vec_capacity_bytes(&batch.n_hits)
        + vec_capacity_bytes(&batch.chip_id)
}

fn roi_spectra_bytes(cache: &RoiSpectraCache) -> u64 {
    cache
        .spectra
        .values()
        .map(|entry| vec_len_bytes::<u64>(entry.data.counts.len()))
        .sum()
}

impl eframe::App for RustpixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply system theme (follows system light/dark preference)
        crate::ui::theme::apply_system_theme(ctx);

        self.handle_messages(ctx);
        self.memory_telemetry.refresh(ctx.input(|i| i.time));

        // Render panels in order: top, bottom, side, central
        self.render_top_panel(ctx);
        self.render_bottom_panel(ctx);
        self.render_side_panel(ctx);
        self.render_central_panel(ctx);
        self.render_settings_windows(ctx);

        if self.processing.is_loading || self.processing.is_processing {
            ctx.request_repaint();
        }
    }
}
