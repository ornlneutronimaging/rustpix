//! Main application state and logic.
//!
//! Contains the `RustpixApp` struct which manages the GUI state,
//! data, and message handling.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use eframe::egui;

use crate::histogram::Hyperstack3D;
use crate::message::AppMessage;
use crate::pipeline::{
    load_file_worker, run_clustering_worker, AlgorithmType, ClusteringWorkerConfig,
};
use crate::state::{ProcessingState, Statistics, UiState, ViewMode};
use crate::viewer::{generate_histogram_image, Colormap};
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;

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
    /// DBSCAN minimum points parameter.
    pub(crate) dbscan_min_points: usize,

    /// Loaded hit batch data.
    pub(crate) hit_batch: Option<HitBatch>,
    /// 3D hyperstack histogram (TOF × Y × X).
    pub(crate) hyperstack: Option<Hyperstack3D>,
    /// Cached 2D projection for visualization (sum over TOF).
    pub(crate) hit_counts: Option<Vec<u64>>,
    /// Cached TOF spectrum (sum over all pixels).
    pub(crate) tof_spectrum: Option<Vec<u64>>,
    /// Extracted neutron events.
    pub(crate) neutrons: NeutronBatch,
    /// 3D hyperstack for neutron data.
    pub(crate) neutron_hyperstack: Option<Hyperstack3D>,
    /// Cached 2D projection for neutron visualization.
    pub(crate) neutron_counts: Option<Vec<u64>>,
    /// Cached TOF spectrum for neutrons.
    pub(crate) neutron_spectrum: Option<Vec<u64>>,
    /// Current cursor info (x, y, hit count).
    pub(crate) cursor_info: Option<(usize, usize, u64)>,

    /// TDC frequency in Hz.
    pub(crate) tdc_frequency: f64,
    /// UI display state.
    pub(crate) ui_state: UiState,

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
}

impl Default for RustpixApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            selected_file: None,
            algo_type: AlgorithmType::Grid, // Default to Grid (Fastest)
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            dbscan_min_points: 2,

            hit_batch: None,
            hyperstack: None,
            hit_counts: None,
            tof_spectrum: None,
            neutrons: NeutronBatch::default(),
            neutron_hyperstack: None,
            neutron_counts: None,
            neutron_spectrum: None,
            cursor_info: None,

            tdc_frequency: 60.0,
            ui_state: UiState::default(),
            rx,
            tx,

            processing: ProcessingState::default(),
            statistics: Statistics::default(),

            texture: None,
            colormap: Colormap::Green,
        }
    }
}

impl RustpixApp {
    /// Load a file asynchronously.
    pub fn load_file(&mut self, path: PathBuf) {
        self.reset_load_state(path.as_path());

        let tx = self.tx.clone();
        let tdc_frequency = self.tdc_frequency;
        thread::spawn(move || load_file_worker(path.as_path(), &tx, tdc_frequency));
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
        self.neutrons.clear();
        self.neutron_hyperstack = None;
        self.neutron_counts = None;
        self.neutron_spectrum = None;
        self.ui_state.view_mode = ViewMode::Hits;
        self.texture = None;
        self.statistics.clear();
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
                dbscan_min_points: self.dbscan_min_points,
                tdc_frequency: self.tdc_frequency,
                total_hits: self.hit_batch.as_ref().map_or(0, HitBatch::len),
            };

            thread::spawn(move || run_clustering_worker(&path, &tx, algo_type, &config));
        }
    }

    /// Get the active hyperstack based on view mode.
    fn active_hyperstack(&self) -> Option<&Hyperstack3D> {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.hyperstack.as_ref(),
            ViewMode::Neutrons => self.neutron_hyperstack.as_ref(),
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

        let Some(counts) = counts else {
            return egui::ColorImage::new([512, 512], egui::Color32::BLACK);
        };
        generate_histogram_image(counts, self.colormap)
    }

    /// Get the cached TOF spectrum (full detector integration).
    pub fn tof_spectrum(&self) -> Option<&[u64]> {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.tof_spectrum.as_deref(),
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

    /// Check if neutron data is available.
    pub fn has_neutrons(&self) -> bool {
        self.neutron_hyperstack.is_some()
    }

    /// Handle pending messages from async workers.
    pub fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::LoadProgress(p, s) | AppMessage::ProcessingProgress(p, s) => {
                    self.processing.progress = p;
                    self.processing.status_text = s;
                }
                AppMessage::LoadComplete(batch, hyperstack, dur, _dbg) => {
                    self.processing.is_loading = false;
                    self.processing.progress = 1.0;
                    self.processing.status_text = "Ready".to_string();

                    // Update statistics
                    self.statistics.hit_count = batch.len();
                    self.statistics.load_duration = Some(dur);
                    self.statistics.tof_max = hyperstack.tof_max();

                    // Cache projections for visualization
                    self.hit_counts = Some(hyperstack.project_xy());
                    self.tof_spectrum = Some(hyperstack.full_spectrum());
                    self.hyperstack = Some(*hyperstack);
                    self.hit_batch = Some(*batch);

                    let img = self.generate_histogram();
                    self.texture =
                        Some(ctx.load_texture("hist", img, egui::TextureOptions::NEAREST));
                }
                AppMessage::LoadError(e) => {
                    self.processing.is_loading = false;
                    self.processing.status_text = format!("Error: {e}");
                }
                AppMessage::ProcessingComplete(neutrons, dur) => {
                    self.processing.is_processing = false;
                    self.processing.progress = 1.0;
                    self.processing.status_text = "Ready".to_string();

                    // Update statistics
                    self.statistics.neutron_count = neutrons.len();
                    self.statistics.cluster_duration = Some(dur);
                    if !neutrons.is_empty() && self.statistics.hit_count > 0 {
                        #[allow(clippy::cast_precision_loss)]
                        {
                            self.statistics.avg_cluster_size =
                                self.statistics.hit_count as f64 / neutrons.len() as f64;
                        }
                    }

                    // Build neutron hyperstack using same TOF parameters as hits
                    if let Some(hit_hs) = &self.hyperstack {
                        let neutron_hs = Hyperstack3D::from_neutrons(
                            &neutrons,
                            hit_hs.n_tof_bins(),
                            hit_hs.tof_max(),
                            hit_hs.width(),
                            hit_hs.height(),
                        );
                        self.neutron_counts = Some(neutron_hs.project_xy());
                        self.neutron_spectrum = Some(neutron_hs.full_spectrum());
                        self.neutron_hyperstack = Some(neutron_hs);
                    }

                    self.neutrons = neutrons;
                }
                AppMessage::ProcessingError(e) => {
                    self.processing.is_processing = false;
                    self.processing.status_text = format!("Error: {e}");
                }
            }
        }
    }
}

impl eframe::App for RustpixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply system theme (follows system light/dark preference)
        crate::ui::theme::apply_system_theme(ctx);

        self.handle_messages(ctx);

        // Render panels in order: top, bottom, side, central
        self.render_top_panel(ctx);
        self.render_bottom_panel(ctx);
        self.render_side_panel(ctx);
        self.render_central_panel(ctx);

        if self.processing.is_loading || self.processing.is_processing {
            ctx.request_repaint();
        }
    }
}
