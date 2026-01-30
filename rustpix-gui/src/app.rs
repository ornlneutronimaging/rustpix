//! Main application state and logic.
//!
//! Contains the `RustpixApp` struct which manages the GUI state,
//! data, and message handling.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use eframe::egui;

use crate::message::AppMessage;
use crate::pipeline::{
    load_file_worker, run_clustering_worker, AlgorithmType, ClusteringWorkerConfig,
};
use crate::state::{ProcessingState, UiState};
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
    /// 512x512 hit count grid for visualization.
    pub(crate) hit_counts: Option<Vec<u32>>,
    /// Extracted neutron events.
    pub(crate) neutrons: NeutronBatch,
    /// Full TOF histogram data.
    pub(crate) tof_hist_full: Option<Vec<u64>>,
    /// Current cursor info (x, y, hit count).
    pub(crate) cursor_info: Option<(usize, usize, u32)>,

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
            hit_counts: None,
            neutrons: NeutronBatch::default(),
            tof_hist_full: None,
            cursor_info: None,

            tdc_frequency: 60.0,
            ui_state: UiState::default(),
            rx,
            tx,

            processing: ProcessingState::default(),

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
        self.processing.status_text.clear();
        self.processing.status_text.push_str("Loading file...");
        self.hit_batch = None;
        self.hit_counts = None;
        self.neutrons.clear();
        self.texture = None;
        self.tof_hist_full = None;
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

    /// Generate histogram image from hit counts.
    pub fn generate_histogram(&self) -> egui::ColorImage {
        let Some(counts) = &self.hit_counts else {
            return egui::ColorImage::new([512, 512], egui::Color32::BLACK);
        };
        generate_histogram_image(counts, self.colormap)
    }

    /// Handle pending messages from async workers.
    pub fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::LoadProgress(p, s) | AppMessage::ProcessingProgress(p, s) => {
                    self.processing.progress = p;
                    self.processing.status_text = s;
                }
                AppMessage::LoadComplete(batch, counts, hist, dur, _dbg) => {
                    self.processing.is_loading = false;
                    self.processing.progress = 1.0;
                    self.processing.status_text =
                        format!("Loaded {} hits in {:.2}s", batch.len(), dur.as_secs_f64());

                    self.hit_counts = Some(counts);
                    self.tof_hist_full = Some(hist);
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
                    self.processing.status_text = format!(
                        "Found {} neutrons in {:.2}ms",
                        neutrons.len(),
                        dur.as_secs_f64() * 1000.0
                    );
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
        self.handle_messages(ctx);
        self.render_side_panel(ctx);
        self.render_central_panel(ctx);
        self.render_histogram_window(ctx);

        if self.processing.is_loading || self.processing.is_processing {
            ctx.request_repaint();
        }
    }
}
